// SPDX-License-Identifier: AGPL-3.0-or-later
//! Private local mailbox and operator contact pages.
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
use bbsbox_mail::{self as mail, Error, Message, ThreadId};
use bbsbox_storage::{Command, disk::Class, writer::Priority};
use serde::Deserialize;
use std::fmt::Write;

const TEMPLATE: &str = include_str!("../../../../../web/shell/compiled/mail.html");

pub(super) fn routes() -> Router<Sessions> {
    Router::new()
        .route(
            "/mail/",
            get(inbox)
                .post(send)
                .layer(DefaultBodyLimit::max(160 * 1024)),
        )
        .route(
            "/mail/contact/",
            get(contact)
                .post(send_contact)
                .layer(DefaultBodyLimit::max(160 * 1024)),
        )
        .route("/mail/{thread}/", get(conversation))
        .route(
            "/mail/{thread}/reply/",
            post(reply).layer(DefaultBodyLimit::max(160 * 1024)),
        )
}

fn render(state: &Sessions, status: StatusCode, title: &str, content: &str) -> Response {
    html(
        status,
        template(
            state,
            TEMPLATE,
            &[("MAIL_TITLE", &escape(title)), ("MAIL_CONTENT", content)],
        ),
    )
}

fn problem(state: &Sessions, status: StatusCode) -> Response {
    if status == StatusCode::UNAUTHORIZED {
        return Redirect::to("/login/").into_response();
    }
    let (title, detail) = match status {
        StatusCode::NOT_FOUND => (
            "Mail unavailable",
            "The conversation or recipient is unavailable.",
        ),
        StatusCode::FORBIDDEN => ("Access refused", "Sign in again before sending mail."),
        StatusCode::BAD_REQUEST
        | StatusCode::UNPROCESSABLE_ENTITY
        | StatusCode::PAYLOAD_TOO_LARGE => (
            "Message not sent",
            "Check the recipient, subject and message length.",
        ),
        StatusCode::CONFLICT => (
            "Message conflicts",
            "Open a fresh form before changing a submitted message.",
        ),
        _ => ("Mail unavailable", "Try again shortly."),
    };
    render(
        state,
        status,
        title,
        &format!("<p>{detail}</p><p><a href=\"/mail/\">Mailbox</a></p>"),
    )
}

async fn read<T: Send + 'static>(
    state: &Sessions,
    work: impl FnOnce(std::path::PathBuf) -> Result<T, Error> + Send + 'static,
) -> Result<T, StatusCode> {
    let permit = state
        .workers
        .clone()
        .try_acquire_owned()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let path = state.database.clone();
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        work(path)
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .map_err(mail_status)
}

#[derive(Deserialize, Default)]
struct Page {
    time: Option<i64>,
    id: Option<String>,
    focus: Option<String>,
}

async fn inbox(
    State(state): State<Sessions>,
    headers: HeaderMap,
    Query(page): Query<Page>,
) -> Response {
    let Some((account, csrf)) = form_access(&state, &headers).await else {
        return problem(&state, StatusCode::UNAUTHORIZED);
    };
    let cursor = match (page.time, page.id.as_deref()) {
        (None, None) => None,
        (Some(time), Some(id)) => match ThreadId::parse(id) {
            Some(id) => Some((time, id)),
            None => return problem(&state, StatusCode::BAD_REQUEST),
        },
        _ => return problem(&state, StatusCode::BAD_REQUEST),
    };
    let result = read(&state, move |path| {
        mail::conversations(&bbsbox_storage::open_reader(path)?, account.id, cursor)
    })
    .await;
    let rows = match result {
        Ok(rows) => rows,
        Err(status) => return problem(&state, status),
    };
    let mut content = String::from("<p><a href=\"/mail/contact/\">Contact a SysOp</a></p>");
    if rows.is_empty() {
        content.push_str("<p>No conversations to show.</p>");
    } else {
        content.push_str("<ul>");
        for row in &rows {
            let _ = write!(
                content,
                "<li><a href=\"/mail/{}/\">{}</a> · {}</li>",
                row.id,
                escape(&row.subject),
                escape(&row.correspondent)
            );
        }
        content.push_str("</ul>");
        if rows.len() == 50 {
            let last = rows.last().expect("full page");
            let _ = write!(
                content,
                "<p><a href=\"?time={}&amp;id={}\">Older conversations</a></p>",
                last.last_activity, last.id
            );
        }
    }
    compose(
        &mut content,
        "/mail/",
        &csrf,
        "<label>Recipient username <input name=\"recipient\" maxlength=\"32\" required></label>",
    );
    render(&state, StatusCode::OK, "Mail", &content)
}

fn compose(content: &mut String, action: &str, csrf: &str, recipient: &str) {
    let fields = format!(
        "{recipient}<label>Subject <input name=\"subject\" maxlength=\"120\" required></label><label>Message <textarea name=\"text\" rows=\"7\" required></textarea></label>"
    );
    form(content, action, csrf, "Send message", &fields);
}

async fn contact(State(state): State<Sessions>, headers: HeaderMap) -> Response {
    let Some((_, csrf)) = form_access(&state, &headers).await else {
        return problem(&state, StatusCode::UNAUTHORIZED);
    };
    let result = read(&state, move |path| {
        bbsbox_identity::operator_usernames(&bbsbox_storage::open_reader(path)?)
            .map_err(Error::Identity)
    })
    .await;
    let names = match result {
        Ok(names) => names,
        Err(status) => return problem(&state, status),
    };
    let mut content = String::from("<p><a href=\"/mail/\">Mailbox</a></p>");
    if names.is_empty() {
        content.push_str("<p>No local operator contacts are available.</p>");
    } else {
        let mut recipients = String::from("<label>SysOp <select name=\"recipient\">");
        for name in names {
            let name = escape(&name);
            let _ = write!(recipients, "<option value=\"{name}\">{name}</option>");
        }
        recipients.push_str("</select></label>");
        compose(&mut content, "/mail/contact/", &csrf, &recipients);
    }
    render(&state, StatusCode::OK, "Contact a SysOp", &content)
}

async fn conversation(
    State(state): State<Sessions>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Query(page): Query<Page>,
) -> Response {
    let Some((account, csrf)) = form_access(&state, &headers).await else {
        return problem(&state, StatusCode::UNAUTHORIZED);
    };
    let Some(thread) = ThreadId::parse(&id) else {
        return problem(&state, StatusCode::NOT_FOUND);
    };
    let focus = match page.focus.as_deref().map(event_id).transpose() {
        Ok(focus) => focus,
        Err(status) => return problem(&state, status),
    };
    let cursor = match (page.time, page.id.as_deref()) {
        (None, None) => None,
        (Some(time), Some(id)) if focus.is_none() => match event_id(id) {
            Ok(id) => Some((time, id)),
            Err(status) => return problem(&state, status),
        },
        _ => return problem(&state, StatusCode::BAD_REQUEST),
    };
    let result = read(&state, move |path| {
        let connection = bbsbox_storage::open_reader(path)?;
        let transaction = connection.unchecked_transaction()?;
        let cursor = match focus {
            Some(target) => mail::before_message(&transaction, account.id, thread, target)?,
            None => cursor,
        };
        let (conversation, messages) = mail::messages(&transaction, account.id, thread, cursor)?;
        let messages = messages
            .into_iter()
            .map(|message| {
                let rendered = bbsbox_discussions::render_markdown(&message.text);
                (message, rendered)
            })
            .collect::<Vec<_>>();
        Ok((conversation, messages))
    })
    .await;
    let (conversation, messages) = match result {
        Ok(result) => result,
        Err(status) => return problem(&state, status),
    };
    let mut content = format!(
        "<p><a href=\"/mail/\">Mailbox</a> · {}</p>",
        escape(&conversation.correspondent)
    );
    if focus.is_some() || cursor.is_some() {
        let _ = write!(
            content,
            "<p><a href=\"/mail/{thread}/\">Start of conversation</a></p>"
        );
    }
    for (message, rendered) in &messages {
        let id = event_key(message.id);
        let _ = write!(
            content,
            "<article id=\"message-{id}\"><h2>{}</h2>",
            escape(&message.sender)
        );
        if let Some(parent) = message.parent {
            let parent = event_key(parent);
            let _ = write!(
                content,
                "<p><a href=\"?focus={parent}#message-{parent}\">Previous message</a></p>"
            );
        }
        let _ = write!(content, "{rendered}</article>");
    }
    if let Some((last, _)) = messages.last() {
        if messages.len() == 30 {
            let _ = write!(
                content,
                "<p><a href=\"?time={}&amp;id={}\">More messages</a></p>",
                last.authored_at,
                event_key(last.id)
            );
        }
        let fields = format!(
            "<input type=\"hidden\" name=\"parent\" value=\"{}\"><label>Reply <textarea name=\"text\" rows=\"6\" required></textarea></label>",
            event_key(last.id)
        );
        form(
            &mut content,
            &format!("/mail/{thread}/reply/"),
            &csrf,
            "Send reply",
            &fields,
        );
    }
    render(&state, StatusCode::OK, &conversation.subject, &content)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Submission {
    #[serde(deserialize_with = "super::password")]
    csrf: zeroize::Zeroizing<String>,
    request: String,
    recipient: Option<String>,
    subject: Option<String>,
    text: String,
    parent: Option<String>,
}

#[derive(Clone, Copy)]
enum Destination {
    Account,
    Operator,
    Thread(ThreadId),
}

async fn send(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    headers: HeaderMap,
    form: Result<Form<Submission>, FormRejection>,
) -> Response {
    submit(state, origin, headers, Destination::Account, form).await
}

async fn send_contact(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    headers: HeaderMap,
    form: Result<Form<Submission>, FormRejection>,
) -> Response {
    submit(state, origin, headers, Destination::Operator, form).await
}

async fn reply(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    Path(id): Path<String>,
    headers: HeaderMap,
    form: Result<Form<Submission>, FormRejection>,
) -> Response {
    match ThreadId::parse(&id) {
        Some(thread) => submit(state, origin, headers, Destination::Thread(thread), form).await,
        None => problem(&state, StatusCode::NOT_FOUND),
    }
}

async fn submit(
    state: Sessions,
    origin: RequestOrigin,
    headers: HeaderMap,
    destination: Destination,
    form: Result<Form<Submission>, FormRejection>,
) -> Response {
    match submit_checked(&state, origin, headers, destination, form).await {
        Ok((thread, message)) => Redirect::to(&format!(
            "/mail/{thread}/?focus={message}#message-{message}"
        ))
        .into_response(),
        Err(status) => problem(&state, status),
    }
}

async fn submit_checked(
    state: &Sessions,
    origin: RequestOrigin,
    headers: HeaderMap,
    destination: Destination,
    form: Result<Form<Submission>, FormRejection>,
) -> Result<(String, String), StatusCode> {
    check_origin(&headers, &origin)?;
    let account = current_account(state, &headers).await?;
    let session = token(&headers)?;
    let Form(input) = form.map_err(|error| error.status())?;
    if input.request.len() != 32 || !input.request.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let text = input.text.replace("\r\n", "\n");
    let parent = input.parent.as_deref().map(event_id).transpose()?;
    let recipient = input.recipient.unwrap_or_default();
    let subject = input.subject.unwrap_or_default();
    let (operation, thread) = match destination {
        Destination::Account => ("send", None),
        Destination::Operator => ("contact", None),
        Destination::Thread(thread) => ("reply", Some(thread.to_string())),
    };
    let fingerprint = serde_json::to_vec(&(
        operation,
        thread,
        parent.map(event_key),
        &recipient,
        &subject,
        &text,
    ))
    .map_err(|_| StatusCode::BAD_REQUEST)?;
    let auth = state.auth.clone();
    let signer = state.signer.clone();
    let mailbox = state.mailbox.clone();
    let receipt = state
        .writer
        .submit_service(Priority::Interactive, move |database| {
            let now = bbsbox_storage::clock::Clock::wall_millis(
                &bbsbox_storage::clock::SystemClock::default(),
            );
            database.transaction_service_checked(
                Command {
                    namespace: &format!("mail:{}", account.id),
                    id: &input.request,
                    request: &fingerprint,
                },
                Class::Content,
                256 * 1024,
                |tx| {
                    let actor = auth
                        .authorize_mutation(tx, &session, &input.csrf, &origin.0)
                        .map_err(Error::Identity)?;
                    let current = auth.mutation_account(tx, &actor).map_err(Error::Identity)?;
                    if current.id != account.id {
                        return Err(Error::Forbidden);
                    }
                    Ok(current.id)
                },
                |tx, account| {
                    let message = match destination {
                        Destination::Thread(thread) => Message::Reply {
                            thread,
                            parent: parent.ok_or(Error::Missing)?,
                            text: &text,
                        },
                        Destination::Account | Destination::Operator => Message::Start {
                            recipient: &recipient,
                            operator_only: matches!(destination, Destination::Operator),
                            subject: &subject,
                            text: &text,
                        },
                    };
                    let (thread, message) = mailbox.send(tx, &signer, account, message, now)?;
                    Ok(format!("{thread}/{}", event_key(message)).into_bytes())
                },
            )
        })
        .map_err(|error| mail_status(error.into()))?
        .wait()
        .await
        .map_err(mail_status)?;
    let value = String::from_utf8(receipt.bytes).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let (thread, message) = value
        .split_once('/')
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    ThreadId::parse(thread).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    event_id(message).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((thread.into(), message.into()))
}

fn mail_status(error: Error) -> StatusCode {
    use bbsbox_storage::Error as Storage;
    match error {
        Error::Identity(error) => super::super::failure(error),
        Error::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        Error::Forbidden => StatusCode::FORBIDDEN,
        Error::Missing => StatusCode::NOT_FOUND,
        Error::Storage(Storage::ReusedCommandId) => StatusCode::CONFLICT,
        Error::Storage(
            Storage::QueueFull
            | Storage::WriterClosed
            | Storage::MaintenancePaused
            | Storage::DiskPressure { .. },
        ) => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
