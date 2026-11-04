// SPDX-License-Identifier: AGPL-3.0-or-later
//! Bounded multipart uploads, authorized attachment downloads and local removal.
use super::{
    Error, RequestOrigin, Sessions, check_origin, escape, files, form_access, login, open_reader,
    password, problem, request_id, status, token,
};
use crate::session::pages::command_nonce;
use axum::{
    Extension, Form, Router,
    body::Body,
    extract::{
        DefaultBodyLimit, Multipart, Path, State,
        multipart::{Field, MultipartRejection},
        rejection::FormRejection,
    },
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use bbsbox_files::{BLOB_NAMESPACE, FileId, MAX_FILE_BYTES, Upload};
use bbsbox_storage::{
    Command,
    blobs::{BlobCommand, Info},
    disk::{Budget, Class, FilesystemSpace, SpaceProbe},
    writer::Priority,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{fmt::Write, io::SeekFrom, path::PathBuf, time::Duration};
use tokio::{
    fs::File,
    io::{AsyncSeekExt, AsyncWriteExt},
};
use tokio_util::io::ReaderStream;
use zeroize::Zeroizing;

const MULTIPART_MAX: usize = MAX_FILE_BYTES as usize + 16 * 1024;

pub(super) fn routes() -> Router<Sessions> {
    Router::new()
        .route(
            "/files/areas/{slug}/upload/",
            post(upload).layer(DefaultBodyLimit::max(MULTIPART_MAX)),
        )
        .route("/files/download/{id}/", get(download))
        .route("/files/remove/{id}/", post(remove))
}

pub(super) fn upload_form(content: &mut String, area: &str, csrf: &str) {
    let request = command_nonce();
    let _ = write!(
        content,
        "<h2>Upload a file</h2><form action=\"/files/areas/{area}/upload/\" method=\"post\" enctype=\"multipart/form-data\"><input type=\"hidden\" name=\"csrf\" value=\"{csrf}\"><input type=\"hidden\" name=\"request\" value=\"{request}\"><label>File <input type=\"file\" name=\"file\" required></label><p>Up to {} MiB per file.</p><button type=\"submit\">Upload</button></form>",
        MAX_FILE_BYTES / (1024 * 1024)
    );
}

async fn small_field(
    multipart: &mut Multipart,
    name: &str,
    limit: usize,
) -> Result<Zeroizing<String>, StatusCode> {
    let mut field = multipart
        .next_field()
        .await
        .map_err(|error| error.status())?
        .ok_or(StatusCode::BAD_REQUEST)?;
    if field.name() != Some(name) || field.file_name().is_some() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut bytes = Zeroizing::new(Vec::new());
    while let Some(chunk) = field.chunk().await.map_err(|error| error.status())? {
        if bytes.len() + chunk.len() > limit {
            return Err(StatusCode::BAD_REQUEST);
        }
        bytes.extend_from_slice(&chunk);
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Zeroizing::new(text.into()))
}

struct Spool {
    filename: String,
    media_type: String,
    info: Info,
    file: std::fs::File,
}

async fn spool_file(multipart: &mut Multipart, directory: PathBuf) -> Result<Spool, StatusCode> {
    let mut field = multipart
        .next_field()
        .await
        .map_err(|error| error.status())?
        .ok_or(StatusCode::BAD_REQUEST)?;
    if field.name() != Some("file") {
        return Err(StatusCode::BAD_REQUEST);
    }
    let filename = field
        .file_name()
        .filter(|name| files::validate_filename(name))
        .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?
        .to_owned();
    let media_type = files::media_type(field.content_type());
    let file = tokio::task::spawn_blocking(move || {
        let available = FilesystemSpace.available(&directory).map_err(io_status)?;
        // At most two spools and two bounded storage copies share this filesystem.
        if available < Budget::default().content_floor + 4 * MAX_FILE_BYTES {
            return Err(StatusCode::INSUFFICIENT_STORAGE);
        }
        tempfile::tempfile_in(directory).map_err(io_status)
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;
    let mut file = File::from_std(file);
    let info = copy_field(&mut field, &mut file).await?;
    drop(field);
    if multipart
        .next_field()
        .await
        .map_err(|error| error.status())?
        .is_some()
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    file.flush().await.map_err(io_status)?;
    file.seek(SeekFrom::Start(0)).await.map_err(io_status)?;
    Ok(Spool {
        filename,
        media_type,
        info,
        file: file.into_std().await,
    })
}

async fn copy_field(field: &mut Field<'_>, file: &mut File) -> Result<Info, StatusCode> {
    let mut size = 0u64;
    let mut hash = Sha256::new();
    while let Some(chunk) = field.chunk().await.map_err(|error| error.status())? {
        size += chunk.len() as u64;
        if size > MAX_FILE_BYTES {
            return Err(StatusCode::PAYLOAD_TOO_LARGE);
        }
        file.write_all(&chunk).await.map_err(io_status)?;
        hash.update(&chunk);
    }
    Ok(Info {
        digest: format!("{:x}", hash.finalize()),
        size,
    })
}

fn io_status(error: std::io::Error) -> StatusCode {
    if error.kind() == std::io::ErrorKind::StorageFull {
        StatusCode::INSUFFICIENT_STORAGE
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

async fn upload(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    multipart: Result<Multipart, MultipartRejection>,
) -> Response {
    let destination = format!("/files/areas/{slug}/");
    let result = receive(&state, origin, slug, headers, multipart).await;
    match result {
        Ok(()) => Redirect::to(&destination).into_response(),
        Err(StatusCode::UNAUTHORIZED) => login(),
        Err(status) => problem(&state, status),
    }
}

async fn receive(
    state: &Sessions,
    origin: RequestOrigin,
    slug: String,
    headers: HeaderMap,
    multipart: Result<Multipart, MultipartRejection>,
) -> Result<(), StatusCode> {
    check_origin(&headers, &origin)?;
    if !files::valid_slug(&slug) {
        return Err(StatusCode::NOT_FOUND);
    }
    let session = token(&headers)?;
    let mut multipart = multipart.map_err(|error| error.into_response().status())?;
    let _permit = state
        .file_uploads
        .clone()
        .try_acquire_owned()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let directory = state
        .database
        .parent()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?
        .to_owned();
    let (csrf, request, actor, spool) = tokio::time::timeout(Duration::from_secs(120), async {
        let csrf = small_field(&mut multipart, "csrf", 64).await?;
        let request = small_field(&mut multipart, "request", 32).await?;
        if !request_id(&request) {
            return Err(StatusCode::BAD_REQUEST);
        }
        let (probe_session, probe_csrf, probe_origin, probe_area) = (
            session.clone(),
            csrf.clone(),
            origin.0.clone(),
            slug.clone(),
        );
        let access = state
            .read(move |auth, path| {
                let reader = open_reader(path)?;
                let tx = reader.unchecked_transaction()?;
                let actor =
                    auth.authorize_mutation(&tx, &probe_session, &probe_csrf, &probe_origin)?;
                let account = auth.mutation_account(&tx, &actor)?;
                Ok(files::allow_upload(&tx, &probe_area, account.id).map(|()| account.id))
            })
            .await?
            .map_err(status)?;
        let spool = spool_file(&mut multipart, directory).await?;
        Ok((csrf, request, access, spool))
    })
    .await
    .map_err(|_| StatusCode::REQUEST_TIMEOUT)??;
    let id = FileId::generate();
    let fingerprint = serde_json::to_vec(&(
        "upload",
        &slug,
        &spool.filename,
        &spool.media_type,
        &spool.info,
    ))
    .map_err(|_| StatusCode::BAD_REQUEST)?;
    let auth = state.auth.clone();
    state
        .writer
        .submit_service(Priority::Interactive, move |database| {
            database.transaction_blob_checked(
                BlobCommand {
                    command: Command {
                        namespace: &format!("files:{actor}"),
                        id: &request,
                        request: &fingerprint,
                    },
                    namespace: BLOB_NAMESPACE,
                    key: &id.to_string(),
                    expected: &spool.info,
                },
                spool.file,
                |tx| {
                    let mutation = auth
                        .authorize_mutation(tx, &session, &csrf, &origin.0)
                        .map_err(Error::Identity)?;
                    let current = auth
                        .mutation_account(tx, &mutation)
                        .map_err(Error::Identity)?;
                    if current.id != actor {
                        return Err(Error::Forbidden);
                    }
                    files::allow_upload(tx, &slug, current.id)?;
                    Ok(current.id)
                },
                |tx, actor, info| {
                    files::publish(
                        tx,
                        actor,
                        Upload {
                            id,
                            area: &slug,
                            filename: &spool.filename,
                            media_type: &spool.media_type,
                        },
                        info,
                    )?;
                    Ok(id.to_string().into_bytes())
                },
            )
        })
        .map_err(|error| status(error.into()))?
        .wait()
        .await
        .map_err(status)?;
    Ok(())
}

async fn download(
    State(state): State<Sessions>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let Some(id) = FileId::parse(&id) else {
        return Ok(problem(&state, StatusCode::NOT_FOUND));
    };
    let session = match token(&headers) {
        Ok(session) => session,
        Err(_) => return Ok(login()),
    };
    let path = state.database.clone();
    let auth = state.auth.clone();
    let result = state
        .writer
        .submit_service(Priority::Interactive, move |database| {
            let reader = open_reader(path)?;
            let tx = reader.unchecked_transaction()?;
            let actor = auth.authenticate(&tx, &session).map_err(Error::Identity)?;
            let entry = files::get(&tx, id, actor.id)?;
            let file = database.get_blob(&entry.digest)?;
            Ok::<_, Error>((entry, file))
        })
        .map_err(|error| status(error.into()))?
        .wait()
        .await;
    let (entry, file) = match result {
        Ok(result) => result,
        Err(error) => {
            let code = status(error);
            return Ok(if code == StatusCode::UNAUTHORIZED {
                login()
            } else {
                problem(&state, code)
            });
        }
    };
    let mut response =
        Body::from_stream(ReaderStream::with_capacity(File::from_std(file), 64 * 1024))
            .into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(entry.size));
    headers.insert(header::CONTENT_DISPOSITION, disposition(&entry.filename));
    Ok(response)
}

fn disposition(filename: &str) -> HeaderValue {
    let mut value = String::from("attachment; filename=\"download\"; filename*=UTF-8''");
    for byte in filename.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_') {
            value.push(char::from(byte));
        } else {
            let _ = write!(value, "%{byte:02X}");
        }
    }
    HeaderValue::from_str(&value).expect("encoded attachment header")
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Remove {
    #[serde(deserialize_with = "password")]
    csrf: Zeroizing<String>,
    request: String,
}

async fn remove(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    Path(id): Path<String>,
    headers: HeaderMap,
    payload: Result<Form<Remove>, FormRejection>,
) -> Result<Response, StatusCode> {
    check_origin(&headers, &origin)?;
    let Some((account, _)) = form_access(&state, &headers).await else {
        return Ok(login());
    };
    let id = FileId::parse(&id).ok_or(StatusCode::NOT_FOUND)?;
    let session = token(&headers)?;
    let Form(change) = payload.map_err(|error| error.status())?;
    if !request_id(&change.request) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let fingerprint = format!("remove:{id}");
    let auth = state.auth.clone();
    let result = state
        .writer
        .submit_service(Priority::Interactive, move |database| {
            let receipt = database.transaction_service_checked(
                Command {
                    namespace: &format!("files:{}", account.id),
                    id: &change.request,
                    request: fingerprint.as_bytes(),
                },
                Class::Control,
                64 * 1024,
                |tx| {
                    let actor = auth
                        .authorize_mutation(tx, &session, &change.csrf, &origin.0)
                        .map_err(Error::Identity)?;
                    let current = auth.mutation_account(tx, &actor).map_err(Error::Identity)?;
                    if current.id != account.id {
                        return Err(Error::Forbidden);
                    }
                    Ok(current.id)
                },
                |tx, actor| Ok(files::remove(tx, id, actor)?.into_bytes()),
            )?;
            if !receipt.replayed {
                // Metadata and reference removal are already durable. Retry reclamation at startup.
                let _ = database.collect_blobs(16);
            }
            Ok::<_, Error>(receipt)
        })
        .map_err(|error| status(error.into()))?
        .wait()
        .await;
    match result {
        Ok(receipt) => {
            let slug = std::str::from_utf8(&receipt.bytes)
                .ok()
                .filter(|slug| files::valid_slug(slug))
                .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
            Ok(Redirect::to(&format!("/files/areas/{}/", escape(slug))).into_response())
        }
        Err(error) => Ok(problem(&state, status(error))),
    }
}
