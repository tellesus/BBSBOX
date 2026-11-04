// SPDX-License-Identifier: AGPL-3.0-or-later
//! Local file areas and SysOp permission settings.
use super::{
    RequestOrigin, Sessions, check_origin, escape, form, form_access, html, password, template,
    token,
};
use axum::{
    Extension, Form, Router,
    extract::{Path, Query, State, rejection::FormRejection},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use bbsbox_files::{self as files, Access, Area, Error, Policy};
use bbsbox_identity::Role;
use bbsbox_storage::{Command, disk::Class, open_reader, writer::Priority};
use serde::Deserialize;
use std::fmt::Write;
use zeroize::Zeroizing;

mod transfers;

const TEMPLATE: &str = include_str!("../../../../../web/shell/compiled/files.html");
const MIB: i64 = 1024 * 1024;

pub(super) fn routes() -> Router<Sessions> {
    Router::new()
        .route("/files/", get(index))
        .route("/files/new/", get(new_area).post(create))
        .route("/files/areas/{slug}/", get(area))
        .route("/files/areas/{slug}/settings/", get(settings).post(change))
        .merge(transfers::routes())
}

fn page(state: &Sessions, status: StatusCode, title: &str, content: &str) -> Response {
    html(
        status,
        template(
            state,
            TEMPLATE,
            &[("FILES_TITLE", &escape(title)), ("FILES_CONTENT", content)],
        ),
    )
}

fn problem(state: &Sessions, status: StatusCode) -> Response {
    let title = match status {
        StatusCode::NOT_FOUND => "File or area unavailable",
        StatusCode::FORBIDDEN => "File access refused",
        StatusCode::CONFLICT => "Review the current area settings",
        StatusCode::PAYLOAD_TOO_LARGE => "File exceeds the upload limit",
        StatusCode::INSUFFICIENT_STORAGE => "There is insufficient space for this upload",
        StatusCode::UNPROCESSABLE_ENTITY | StatusCode::BAD_REQUEST => {
            "Check the file or area details"
        }
        StatusCode::REQUEST_TIMEOUT => "The upload timed out",
        _ => "Files unavailable",
    };
    page(state, status, title, "<p><a href=\"/files/\">Files</a></p>")
}

fn login() -> Response {
    Redirect::to("/login/").into_response()
}

fn request_id(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[derive(Deserialize)]
struct Areas {
    after: Option<String>,
}

async fn index(
    State(state): State<Sessions>,
    headers: HeaderMap,
    Query(query): Query<Areas>,
) -> Result<Response, StatusCode> {
    let Some((account, _)) = form_access(&state, &headers).await else {
        return Ok(login());
    };
    if query
        .after
        .as_deref()
        .is_some_and(|slug| !files::valid_slug(slug))
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let rows = state
        .read(move |_, path| {
            Ok(files::list_areas(
                &open_reader(path)?,
                account.id,
                query.after.as_deref(),
            ))
        })
        .await?
        .map_err(status)?;
    let mut content = String::new();
    if account.role == Role::Sysop {
        content.push_str("<p><a href=\"/files/new/\">Create a file area</a></p>");
    }
    if rows.is_empty() {
        content.push_str("<p>No file areas to show.</p>");
    } else {
        content.push_str("<ul>");
        for row in rows.iter().take(files::PAGE_SIZE) {
            let _ = write!(
                content,
                "<li><a href=\"/files/areas/{}/\">{}</a></li>",
                row.slug,
                escape(&row.policy.title)
            );
        }
        content.push_str("</ul>");
        if let Some(last) = rows
            .get(files::PAGE_SIZE - 1)
            .filter(|_| rows.len() > files::PAGE_SIZE)
        {
            let _ = write!(
                content,
                "<p><a href=\"?after={}\">More areas</a></p>",
                last.slug
            );
        }
    }
    Ok(page(&state, StatusCode::OK, "Files", &content))
}

#[derive(Deserialize)]
struct Entries {
    before: Option<i64>,
}

async fn area(
    State(state): State<Sessions>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    Query(query): Query<Entries>,
) -> Result<Response, StatusCode> {
    let Some((account, csrf)) = form_access(&state, &headers).await else {
        return Ok(login());
    };
    if !files::valid_slug(&slug) {
        return Ok(problem(&state, StatusCode::NOT_FOUND));
    }
    let result = state
        .read(move |_, path| {
            let reader = open_reader(path)?;
            let tx = reader.unchecked_transaction()?;
            Ok(
                files::read_area(&tx, &slug, account.id).and_then(|(area, viewer)| {
                    let rows = files::list(&tx, &slug, viewer.id, query.before)?;
                    Ok((area, viewer, rows))
                }),
            )
        })
        .await?;
    let (area, account, rows) = match result {
        Ok(value) => value,
        Err(error) => return Ok(problem(&state, status(error))),
    };
    let slug = &area.slug;
    let mut content = String::from("<p><a href=\"/files/\">File areas</a></p>");
    if account.role == Role::Sysop {
        let _ = write!(
            content,
            "<p><a href=\"/files/areas/{slug}/settings/\">Area settings</a></p>"
        );
    }
    let _ = write!(
        content,
        "<p>{} of {} bytes used.</p>",
        area.used, area.policy.quota
    );
    if area.policy.upload.allows(&account) {
        transfers::upload_form(&mut content, slug, &csrf);
    }
    if rows.is_empty() {
        content.push_str("<p>No files to show.</p>");
    } else {
        for entry in rows.iter().take(files::PAGE_SIZE) {
            let id = entry.id;
            let _ = write!(
                content,
                "<article><h2><a href=\"/files/download/{id}/\">{}</a></h2><p>{} bytes · {}</p>",
                escape(&entry.filename),
                entry.size,
                escape(&entry.media_type)
            );
            if account.role == Role::Sysop || account.id.bytes() == &entry.owner {
                form(
                    &mut content,
                    &format!("/files/remove/{id}/"),
                    &csrf,
                    "Remove file",
                    "",
                );
            }
            content.push_str("</article>");
        }
        if let Some(last) = rows
            .get(files::PAGE_SIZE - 1)
            .filter(|_| rows.len() > files::PAGE_SIZE)
        {
            let _ = write!(
                content,
                "<p><a href=\"?before={}\">More files</a></p>",
                last.sequence
            );
        }
    }
    Ok(page(&state, StatusCode::OK, &area.policy.title, &content))
}

fn editor(state: &Sessions, csrf: &str, area: Option<Area>) -> Response {
    let mut fields = String::new();
    let (path, label, title, download, upload, quota) = if let Some(area) = area {
        let _ = write!(
            fields,
            "<input type=\"hidden\" name=\"revision\" value=\"{}\">",
            area.revision
        );
        (
            format!("/files/areas/{}/settings/", area.slug),
            "Save area",
            area.policy.title,
            area.policy.download,
            area.policy.upload,
            area.policy.quota / MIB,
        )
    } else {
        fields.push_str("<label>Area address <input name=\"slug\" pattern=\"[a-z0-9\\-]{1,32}\" maxlength=\"32\" required></label>");
        (
            "/files/new/".into(),
            "Create area",
            String::new(),
            Access::Members,
            Access::Sysops,
            64,
        )
    };
    let _ = write!(
        fields,
        "<label>Title <input name=\"title\" maxlength=\"120\" value=\"{}\" required></label><label>Area quota (MiB) <input type=\"number\" name=\"quota\" min=\"1\" max=\"1024\" value=\"{quota}\" required></label>",
        escape(&title)
    );
    for (key, label, selected) in [
        ("download", "Who can download", download),
        ("upload", "Who can upload", upload),
    ] {
        let _ = write!(fields, "<label>{label} <select name=\"{key}\">");
        for (access, label) in [
            (Access::Members, "All signed-in accounts"),
            (Access::Sysops, "SysOps only"),
        ] {
            let marker = if selected == access { " selected" } else { "" };
            let _ = write!(
                fields,
                "<option value=\"{}\"{marker}>{label}</option>",
                access.key()
            );
        }
        fields.push_str("</select></label>");
    }
    let mut content = String::from("<p><a href=\"/files/\">File areas</a></p>");
    form(&mut content, &path, csrf, label, &fields);
    page(state, StatusCode::OK, "File area settings", &content)
}

async fn new_area(State(state): State<Sessions>, headers: HeaderMap) -> Response {
    match form_access(&state, &headers).await {
        Some((account, csrf)) if account.role == Role::Sysop => editor(&state, &csrf, None),
        Some(_) => problem(&state, StatusCode::FORBIDDEN),
        None => login(),
    }
}

async fn settings(
    State(state): State<Sessions>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let Some((account, csrf)) = form_access(&state, &headers).await else {
        return Ok(login());
    };
    if account.role != Role::Sysop {
        return Ok(problem(&state, StatusCode::FORBIDDEN));
    }
    match state
        .read(move |_, path| Ok(files::read_area(&open_reader(path)?, &slug, account.id)))
        .await?
    {
        Ok((area, _)) => Ok(editor(&state, &csrf, Some(area))),
        Err(error) => Ok(problem(&state, status(error))),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
    #[serde(deserialize_with = "password")]
    csrf: Zeroizing<String>,
    request: String,
    slug: Option<String>,
    revision: Option<i64>,
    title: String,
    download: String,
    upload: String,
    quota: i64,
}

async fn create(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    headers: HeaderMap,
    input: Result<Form<Change>, FormRejection>,
) -> Result<Response, StatusCode> {
    save(state, origin, headers, None, input).await
}

async fn change(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    input: Result<Form<Change>, FormRejection>,
) -> Result<Response, StatusCode> {
    save(state, origin, headers, Some(slug), input).await
}

async fn save(
    state: Sessions,
    origin: RequestOrigin,
    headers: HeaderMap,
    slug: Option<String>,
    input: Result<Form<Change>, FormRejection>,
) -> Result<Response, StatusCode> {
    check_origin(&headers, &origin)?;
    let Some((account, _)) = form_access(&state, &headers).await else {
        return Ok(login());
    };
    if account.role != Role::Sysop {
        return Ok(problem(&state, StatusCode::FORBIDDEN));
    }
    let session = token(&headers)?;
    let Form(change) = input.map_err(|error| error.status())?;
    if !request_id(&change.request) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let slug = match (slug, change.slug, change.revision) {
        (None, Some(slug), None) | (Some(slug), None, Some(_)) if files::valid_slug(&slug) => slug,
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    let policy = Policy {
        title: change.title.trim().into(),
        download: Access::parse(&change.download).ok_or(StatusCode::BAD_REQUEST)?,
        upload: Access::parse(&change.upload).ok_or(StatusCode::BAD_REQUEST)?,
        quota: change
            .quota
            .checked_mul(MIB)
            .ok_or(StatusCode::BAD_REQUEST)?,
    };
    let fingerprint = serde_json::to_vec(&(
        "area",
        &slug,
        change.revision,
        &policy.title,
        policy.download.key(),
        policy.upload.key(),
        policy.quota,
    ))
    .map_err(|_| StatusCode::BAD_REQUEST)?;
    let auth = state.auth.clone();
    let destination = format!("/files/areas/{slug}/");
    let result = state
        .writer
        .submit_service(Priority::Interactive, move |database| {
            database.transaction_service_checked(
                Command {
                    namespace: &format!("files:{}", account.id),
                    id: &change.request,
                    request: &fingerprint,
                },
                Class::Control,
                64 * 1024,
                |tx| {
                    let actor = auth
                        .authorize_mutation(tx, &session, &change.csrf, &origin.0)
                        .map_err(Error::Identity)?;
                    let current = auth.require_sysop(tx, &actor).map_err(Error::Identity)?;
                    if current.id != account.id {
                        return Err(Error::Forbidden);
                    }
                    Ok(current.id)
                },
                |tx, actor| {
                    files::configure(tx, &slug, actor, change.revision, &policy)?;
                    Ok(Vec::new())
                },
            )
        })
        .map_err(|error| status(error.into()))?
        .wait()
        .await;
    Ok(match result {
        Ok(_) => Redirect::to(&destination).into_response(),
        Err(error) => problem(&state, status(error)),
    })
}

fn status(error: Error) -> StatusCode {
    use bbsbox_storage::Error as Storage;
    match error {
        Error::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        Error::Forbidden => StatusCode::FORBIDDEN,
        Error::Missing => StatusCode::NOT_FOUND,
        Error::Quota | Error::Storage(Storage::DiskPressure { .. }) => {
            StatusCode::INSUFFICIENT_STORAGE
        }
        Error::Storage(Storage::BlobTooLarge) => StatusCode::PAYLOAD_TOO_LARGE,
        Error::Conflict | Error::Storage(Storage::ReusedCommandId) => StatusCode::CONFLICT,
        Error::Identity(error) => super::super::failure(error),
        Error::Storage(Storage::QueueFull | Storage::WriterClosed | Storage::MaintenancePaused) => {
            StatusCode::SERVICE_UNAVAILABLE
        }
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
