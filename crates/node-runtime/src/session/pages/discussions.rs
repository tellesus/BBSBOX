// SPDX-License-Identifier: AGPL-3.0-or-later
//! Browser entry for local signed discussion boards.
use super::{
    RequestOrigin, Sessions, check_origin, current_account, escape, event_id, event_key, form,
    form_access, html, template, token,
};
use axum::{
    Extension, Form, Router,
    extract::{DefaultBodyLimit, Path, Query, State, rejection::FormRejection},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use bbsbox_discussions::{self as boards, Change};
use bbsbox_event_codec::EventId;
use bbsbox_identity::Role;
use bbsbox_storage::{Command, disk::Class, writer::Priority};
use serde::Deserialize;
use std::fmt::Write;

const TEMPLATE: &str = include_str!("../../../../../web/shell/compiled/discussions.html");
const PAGE_SIZE: usize = 100;

pub(super) fn routes() -> Router<Sessions> {
    Router::new()
        .route("/discussions/", get(list).post(create_board))
        .route("/discussions/{board}/", get(board))
        .route("/discussions/{board}/{thread}/", get(thread))
        .route(
            "/discussions/{board}/new/",
            post(create_thread).layer(DefaultBodyLimit::max(160 * 1024)),
        )
        .route(
            "/discussions/{board}/{thread}/reply/",
            post(reply).layer(DefaultBodyLimit::max(160 * 1024)),
        )
        .route(
            "/discussions/{board}/{thread}/{post}/edit/",
            get(editor)
                .post(edit)
                .layer(DefaultBodyLimit::max(160 * 1024)),
        )
        .route(
            "/discussions/{board}/{thread}/{post}/withdraw/",
            post(withdraw).layer(DefaultBodyLimit::max(160 * 1024)),
        )
}

fn render(state: &Sessions, status: StatusCode, title: &str, content: &str) -> Response {
    html(
        status,
        template(
            state,
            TEMPLATE,
            &[
                ("DISCUSSIONS_TITLE", &escape(title)),
                ("DISCUSSIONS_CONTENT", content),
            ],
        ),
    )
}

fn problem(state: &Sessions, status: StatusCode) -> Response {
    if status == StatusCode::UNAUTHORIZED {
        return Redirect::to("/login/").into_response();
    }
    let (title, message) = match status {
        StatusCode::NOT_FOUND => (
            "Discussion unavailable",
            "The board, thread, or reply is unavailable.",
        ),
        StatusCode::CONFLICT => (
            "Changes conflict",
            "This board already exists or the form has already been used. Open a fresh form and try again.",
        ),
        StatusCode::FORBIDDEN => (
            "Access refused",
            "Check that you are signed in with the required access.",
        ),
        StatusCode::BAD_REQUEST
        | StatusCode::UNPROCESSABLE_ENTITY
        | StatusCode::PAYLOAD_TOO_LARGE => (
            "Post not accepted",
            "Check the fields and keep the post within the length limit.",
        ),
        _ => ("Discussions unavailable", "Try again shortly."),
    };
    let body = format!("<p>{message}</p><p><a href=\"/discussions/\">Discussions</a></p>");
    render(state, status, title, &body)
}

async fn read<T: Send + 'static>(
    state: &Sessions,
    operation: impl FnOnce(std::path::PathBuf) -> Result<T, boards::Error> + Send + 'static,
) -> Result<T, StatusCode> {
    let permit = state
        .workers
        .clone()
        .try_acquire_owned()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let path = state.database.clone();
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        operation(path)
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .map_err(board_status)
}

#[derive(Deserialize, Default)]
struct Page {
    after: Option<String>,
    time: Option<i64>,
    id: Option<String>,
    focus: Option<String>,
}

impl Page {
    fn cursor(&self) -> Result<Option<(i64, EventId)>, StatusCode> {
        match (self.time, self.id.as_deref()) {
            (None, None) => Ok(None),
            (Some(time), Some(id)) => Ok(Some((time, event_id(id)?))),
            _ => Err(StatusCode::BAD_REQUEST),
        }
    }
}

async fn list(
    State(state): State<Sessions>,
    headers: HeaderMap,
    Query(page): Query<Page>,
) -> Response {
    let result = read(&state, move |path| {
        boards::boards(&bbsbox_storage::open_reader(path)?, page.after.as_deref())
    })
    .await;
    let rows = match result {
        Ok(rows) => rows,
        Err(status) => return problem(&state, status),
    };
    let mut content = String::new();
    if rows.is_empty() {
        content.push_str("<p>No boards to show.</p>");
    } else {
        content.push_str("<ul>");
        for row in &rows {
            let _ = write!(
                content,
                "<li><a href=\"/discussions/{}/\">{}</a></li>",
                row.slug,
                escape(&row.title)
            );
        }
        content.push_str("</ul>");
        if rows.len() == PAGE_SIZE {
            let _ = write!(
                content,
                "<p><a href=\"?after={}\">More boards</a></p>",
                rows.last().expect("full page").slug
            );
        }
    }
    if let Some((account, csrf)) = form_access(&state, &headers).await
        && account.role == Role::Sysop
    {
        form(
            &mut content,
            "/discussions/",
            &csrf,
            "Create board",
            "<label>Board title <input name=\"title\" maxlength=\"120\" required></label><label>Board address <input name=\"board\" pattern=\"[a-z0-9-]+\" maxlength=\"32\" required></label>",
        );
    }
    render(&state, StatusCode::OK, "Discussions", &content)
}

async fn board(
    State(state): State<Sessions>,
    Path(board): Path<String>,
    headers: HeaderMap,
    Query(page): Query<Page>,
) -> Response {
    let cursor = match page.cursor() {
        Ok(cursor) => cursor,
        Err(status) => return problem(&state, status),
    };
    let lookup = board.clone();
    let result = read(&state, move |path| {
        let connection = bbsbox_storage::open_reader(path)?;
        Ok((
            boards::board_title(&connection, &lookup)?,
            boards::threads(&connection, &lookup, cursor)?,
        ))
    })
    .await;
    let (title, rows) = match result {
        Ok(result) => result,
        Err(status) => return problem(&state, status),
    };
    let mut content = String::from("<p><a href=\"/discussions/\">All boards</a></p>");
    if rows.is_empty() {
        content.push_str("<p>No threads to show.</p>");
    } else {
        content.push_str("<ul>");
        for row in &rows {
            let _ = write!(
                content,
                "<li><a href=\"/discussions/{board}/{}/\">{}</a> · {}</li>",
                event_key(row.id),
                escape(&row.title),
                escape(&row.username)
            );
        }
        content.push_str("</ul>");
        if rows.len() == PAGE_SIZE {
            let last = rows.last().expect("full page");
            let _ = write!(
                content,
                "<p><a href=\"?time={}&amp;id={}\">Older threads</a></p>",
                last.created_at,
                event_key(last.id)
            );
        }
    }
    if let Some((_, csrf)) = form_access(&state, &headers).await {
        form(
            &mut content,
            &format!("/discussions/{board}/new/"),
            &csrf,
            "Start thread",
            "<label>Title <input name=\"title\" maxlength=\"120\" required></label><label>Post <textarea name=\"text\" rows=\"6\" required></textarea></label>",
        );
    } else {
        content.push_str("<p><a href=\"/login/\">Sign in</a> to start a thread.</p>");
    }
    render(&state, StatusCode::OK, &title, &content)
}

async fn thread(
    State(state): State<Sessions>,
    Path((board, key)): Path<(String, String)>,
    headers: HeaderMap,
    Query(page): Query<Page>,
) -> Response {
    let (id, cursor) = match (event_id(&key), page.cursor()) {
        (Ok(id), Ok(cursor)) => (id, cursor),
        (Err(status), _) | (_, Err(status)) => return problem(&state, status),
    };
    let focus = match page.focus.as_deref().map(event_id).transpose() {
        Ok(focus) if focus.is_none() || cursor.is_none() => focus,
        Ok(_) => return problem(&state, StatusCode::BAD_REQUEST),
        Err(status) => return problem(&state, status),
    };
    let lookup = board.clone();
    let result = read(&state, move |path| {
        let connection = bbsbox_storage::open_reader(path)?;
        let title = boards::thread_title(&connection, &lookup, id)?;
        let cursor = match focus {
            Some(focus) => boards::before_post(&connection, id, focus)?,
            None => cursor,
        };
        let posts = boards::posts(&connection, &lookup, id, cursor)?
            .into_iter()
            .map(|post| {
                let rendered = if post.withdrawn {
                    "<p>Post withdrawn.</p>".to_owned()
                } else {
                    boards::render_markdown(&post.text)
                };
                (post, rendered)
            })
            .collect::<Vec<_>>();
        Ok((title, posts))
    })
    .await;
    let (title, rows) = match result {
        Ok(result) => result,
        Err(status) => return problem(&state, status),
    };
    let mut content = format!("<p><a href=\"/discussions/{board}/\">Back to board</a></p>");
    if focus.is_some() || cursor.is_some() {
        let _ = write!(
            content,
            "<p><a href=\"/discussions/{board}/{key}/\">Start of thread</a></p>"
        );
    }
    let mut parents = format!("<option value=\"{key}\">Thread opener</option>");
    let access = form_access(&state, &headers).await;
    for (row, rendered) in &rows {
        let post = event_key(row.id);
        let _ = write!(
            content,
            "<article id=\"post-{post}\"><h2>{}</h2>",
            escape(&row.username)
        );
        if let Some(parent) = row.parent {
            let parent_key = event_key(parent);
            let _ = write!(
                content,
                "<p><a href=\"?focus={parent_key}#post-{parent_key}\">Parent post</a></p>"
            );
            let _ = write!(
                parents,
                "<option value=\"{post}\">{} · {}</option>",
                escape(&row.username),
                &post[..8]
            );
        }
        content.push_str(rendered);
        if !row.withdrawn
            && access
                .as_ref()
                .is_some_and(|(account, _)| account.username == row.username)
        {
            let _ = write!(
                content,
                "<p><a href=\"/discussions/{board}/{key}/{post}/edit/\">Edit or withdraw post</a></p>"
            );
        }
        content.push_str("</article>");
    }
    if rows.len() == boards::POST_PAGE_SIZE {
        let last = &rows.last().expect("full page").0;
        let _ = write!(
            content,
            "<p><a href=\"?time={}&amp;id={}\">More replies</a></p>",
            last.created_at,
            event_key(last.id)
        );
    }
    if let Some((_, csrf)) = access {
        let fields = format!(
            "<label>Reply to <select name=\"parent\">{parents}</select></label><label>Reply <textarea name=\"text\" rows=\"6\" required></textarea></label>"
        );
        form(
            &mut content,
            &format!("/discussions/{board}/{key}/reply/"),
            &csrf,
            "Post reply",
            &fields,
        );
    } else {
        content.push_str("<p><a href=\"/login/\">Sign in</a> to reply.</p>");
    }
    render(&state, StatusCode::OK, &title, &content)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Submission {
    #[serde(deserialize_with = "super::password")]
    csrf: zeroize::Zeroizing<String>,
    request: String,
    board: Option<String>,
    title: Option<String>,
    text: Option<String>,
    parent: Option<String>,
    previous: Option<String>,
}

#[derive(Clone, Copy)]
enum Kind {
    Board,
    Thread,
    Reply(EventId),
    Edit(EventId, EventId),
    Withdraw(EventId, EventId),
}

async fn editor(
    State(state): State<Sessions>,
    Path((board, thread, post)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    let Some((account, csrf)) = form_access(&state, &headers).await else {
        return problem(&state, StatusCode::UNAUTHORIZED);
    };
    let (Ok(thread_id), Ok(post_id)) = (event_id(&thread), event_id(&post)) else {
        return problem(&state, StatusCode::NOT_FOUND);
    };
    let lookup = board.clone();
    let result = read(&state, move |path| {
        boards::current_post(
            &bbsbox_storage::open_reader(path)?,
            &lookup,
            thread_id,
            post_id,
        )
    })
    .await;
    let current = match result {
        Ok(post) => post,
        Err(status) => return problem(&state, status),
    };
    if current.username != account.username {
        return problem(&state, StatusCode::FORBIDDEN);
    }
    if current.withdrawn {
        return problem(&state, StatusCode::CONFLICT);
    }
    let target = format!("/discussions/{board}/{thread}/{post}");
    let revision = format!(
        "<input type=\"hidden\" name=\"previous\" value=\"{}\">",
        event_key(current.revision)
    );
    let fields = format!(
        "{revision}<label>Post <textarea name=\"text\" rows=\"8\" required>\n{}</textarea></label>",
        escape(&current.text)
    );
    let mut content = format!(
        "<p><a href=\"/discussions/{board}/{thread}/?focus={post}#post-{post}\">Return to post</a></p>"
    );
    form(
        &mut content,
        &format!("{target}/edit/"),
        &csrf,
        "Save edit",
        &fields,
    );
    form(
        &mut content,
        &format!("{target}/withdraw/"),
        &csrf,
        "Withdraw post",
        &revision,
    );
    render(&state, StatusCode::OK, "Edit post", &content)
}

async fn edit(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    Path((board, thread, post)): Path<(String, String, String)>,
    headers: HeaderMap,
    form: Result<Form<Submission>, FormRejection>,
) -> Response {
    match (event_id(&thread), event_id(&post)) {
        (Ok(thread), Ok(post)) => {
            write_form(
                state,
                origin,
                headers,
                board,
                Kind::Edit(thread, post),
                form,
            )
            .await
        }
        _ => problem(&state, StatusCode::NOT_FOUND),
    }
}

async fn withdraw(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    Path((board, thread, post)): Path<(String, String, String)>,
    headers: HeaderMap,
    form: Result<Form<Submission>, FormRejection>,
) -> Response {
    match (event_id(&thread), event_id(&post)) {
        (Ok(thread), Ok(post)) => {
            write_form(
                state,
                origin,
                headers,
                board,
                Kind::Withdraw(thread, post),
                form,
            )
            .await
        }
        _ => problem(&state, StatusCode::NOT_FOUND),
    }
}

async fn create_board(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    headers: HeaderMap,
    form: Result<Form<Submission>, FormRejection>,
) -> Response {
    let board = form
        .as_ref()
        .ok()
        .and_then(|form| form.board.clone())
        .unwrap_or_default();
    write_form(state, origin, headers, board, Kind::Board, form).await
}

async fn create_thread(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    Path(board): Path<String>,
    headers: HeaderMap,
    form: Result<Form<Submission>, FormRejection>,
) -> Response {
    write_form(state, origin, headers, board, Kind::Thread, form).await
}

async fn reply(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    Path((board, thread)): Path<(String, String)>,
    headers: HeaderMap,
    form: Result<Form<Submission>, FormRejection>,
) -> Response {
    match event_id(&thread) {
        Ok(thread) => write_form(state, origin, headers, board, Kind::Reply(thread), form).await,
        Err(status) => problem(&state, status),
    }
}

async fn write_form(
    state: Sessions,
    origin: RequestOrigin,
    headers: HeaderMap,
    board: String,
    kind: Kind,
    form: Result<Form<Submission>, FormRejection>,
) -> Response {
    match write_checked(&state, origin, headers, &board, kind, form).await {
        Ok(id) => {
            let target = match kind {
                Kind::Board => format!("/discussions/{board}/"),
                Kind::Thread => format!("/discussions/{board}/{}/", event_key(id)),
                Kind::Reply(thread) => {
                    let reply = event_key(id);
                    format!(
                        "/discussions/{board}/{}/?focus={reply}#post-{reply}",
                        event_key(thread)
                    )
                }
                Kind::Edit(thread, post) | Kind::Withdraw(thread, post) => {
                    let post = event_key(post);
                    format!(
                        "/discussions/{board}/{}/?focus={post}#post-{post}",
                        event_key(thread)
                    )
                }
            };
            Redirect::to(&target).into_response()
        }
        Err(status) => problem(&state, status),
    }
}

async fn write_checked(
    state: &Sessions,
    origin: RequestOrigin,
    headers: HeaderMap,
    board: &str,
    kind: Kind,
    form: Result<Form<Submission>, FormRejection>,
) -> Result<EventId, StatusCode> {
    check_origin(&headers, &origin)?;
    let account = current_account(state, &headers).await?;
    let session = token(&headers)?;
    let Form(input) = form.map_err(|error| error.status())?;
    if input.request.len() != 32 || !input.request.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let title = input.title.unwrap_or_default();
    let text = input.text.unwrap_or_default().replace("\r\n", "\n");
    let parent = input.parent.as_deref().map(event_id).transpose()?;
    let previous = input.previous.as_deref().map(event_id).transpose()?;
    let operation = match kind {
        Kind::Board => "board",
        Kind::Thread => "thread",
        Kind::Reply(_) => "reply",
        Kind::Edit(_, _) => "edit",
        Kind::Withdraw(_, _) => "withdraw",
    };
    let route_thread = match kind {
        Kind::Reply(id) => Some(event_key(id)),
        _ => None,
    };
    let fingerprint = match kind {
        Kind::Edit(thread, post) | Kind::Withdraw(thread, post) => serde_json::to_vec(&(
            operation,
            board,
            event_key(thread),
            event_key(post),
            previous.map(event_key),
            &text,
        )),
        _ => serde_json::to_vec(&(
            operation,
            board,
            &title,
            &text,
            route_thread,
            parent.map(event_key),
        )),
    }
    .map_err(|_| StatusCode::BAD_REQUEST)?;
    let auth = state.auth.clone();
    let signer = state.signer.clone();
    let service = state.boards.clone();
    let board = board.to_owned();
    let receipt = state
        .writer
        .submit_service(Priority::Interactive, move |database| {
            let now = bbsbox_storage::clock::Clock::wall_millis(
                &bbsbox_storage::clock::SystemClock::default(),
            );
            database.transaction_service_checked(
                Command {
                    namespace: &format!("discussions:{}", account.id),
                    id: &input.request,
                    request: &fingerprint,
                },
                Class::Content,
                256 * 1024,
                |tx| {
                    let actor = auth
                        .authorize_mutation(tx, &session, &input.csrf, &origin.0)
                        .map_err(boards::Error::Identity)?;
                    let current = auth
                        .mutation_account(tx, &actor)
                        .map_err(boards::Error::Identity)?;
                    if current.id != account.id
                        || (matches!(kind, Kind::Board) && current.role != Role::Sysop)
                    {
                        return Err(boards::Error::Forbidden);
                    }
                    Ok(current)
                },
                |tx, account| {
                    let change = match kind {
                        Kind::Board => Change::Board { title: &title },
                        Kind::Thread => Change::Thread {
                            title: &title,
                            text: &text,
                        },
                        Kind::Reply(thread) => {
                            let parent = parent.ok_or(boards::Error::MissingParent)?;
                            if boards::reply_thread(tx, &board, parent)? != thread {
                                return Err(boards::Error::MissingParent);
                            }
                            Change::Reply {
                                parent,
                                text: &text,
                            }
                        }
                        Kind::Edit(thread, post) | Kind::Withdraw(thread, post) => {
                            if boards::reply_thread(tx, &board, post)? != thread {
                                return Err(boards::Error::MissingParent);
                            }
                            let previous = previous.ok_or(boards::Error::RevisionChanged)?;
                            if matches!(kind, Kind::Edit(_, _)) {
                                Change::Edit {
                                    post,
                                    previous,
                                    text: &text,
                                }
                            } else {
                                Change::Withdraw { post, previous }
                            }
                        }
                    };
                    service
                        .record(tx, &signer, &account, &board, change, now)
                        .map(|id| id.bytes().to_vec())
                },
            )
        })
        .map_err(|error| board_status(error.into()))?
        .wait()
        .await
        .map_err(board_status)?;
    Ok(EventId::from_bytes(
        receipt
            .bytes
            .try_into()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    ))
}

fn board_status(error: boards::Error) -> StatusCode {
    use bbsbox_storage::Error as Storage;
    match error {
        boards::Error::Identity(error) => super::super::failure(error),
        boards::Error::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        boards::Error::Forbidden => StatusCode::FORBIDDEN,
        boards::Error::MissingBoard | boards::Error::MissingParent => StatusCode::NOT_FOUND,
        boards::Error::BoardExists
        | boards::Error::RevisionChanged
        | boards::Error::NoChange
        | boards::Error::Storage(Storage::ReusedCommandId) => StatusCode::CONFLICT,
        boards::Error::Storage(
            Storage::QueueFull
            | Storage::WriterClosed
            | Storage::MaintenancePaused
            | Storage::DiskPressure { .. },
        ) => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
