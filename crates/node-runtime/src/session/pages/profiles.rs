// SPDX-License-Identifier: AGPL-3.0-or-later
//! Local account directory, published profile and owner editor.
use super::{
    RequestOrigin, Sessions, check_origin, escape, form_access, html, password, template, token,
};
use axum::{
    Extension, Form, Router,
    extract::{DefaultBodyLimit, Path, Query, State, rejection::FormRejection},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use bbsbox_profiles::{self as profiles, Error, Field, Preferences};
use bbsbox_storage::{disk::Class, open_reader};
use serde::Deserialize;
use std::fmt::Write;
use zeroize::Zeroizing;

const TEMPLATE: &str = include_str!("../../../../../web/shell/compiled/directory.html");

pub(super) fn routes() -> Router<Sessions> {
    Router::new()
        .route("/directory/", get(directory))
        .route("/directory/{username}/", get(profile))
        .route(
            "/account/profile/",
            get(editor)
                .post(save)
                .layer(DefaultBodyLimit::max(48 * 1024)),
        )
}

fn page(state: &Sessions, status: StatusCode, title: &str, content: &str) -> Response {
    html(
        status,
        template(
            state,
            TEMPLATE,
            &[
                ("DIRECTORY_TITLE", &escape(title)),
                ("DIRECTORY_CONTENT", content),
            ],
        ),
    )
}

fn unavailable(state: &Sessions) -> Response {
    page(
        state,
        StatusCode::NOT_FOUND,
        "Account unavailable",
        "<p><a href=\"/directory/\">Directory</a></p>",
    )
}

#[derive(Deserialize)]
struct Browse {
    after: Option<String>,
    username: Option<String>,
}

async fn directory(
    State(state): State<Sessions>,
    headers: HeaderMap,
    Query(query): Query<Browse>,
) -> Result<Response, StatusCode> {
    if form_access(&state, &headers).await.is_none() {
        return Ok(Redirect::to("/login/").into_response());
    }
    if let Some(username) = query.username.filter(|name| !name.trim().is_empty()) {
        return Ok(match bbsbox_identity::normalize_username(username.trim()) {
            Ok(username) => Redirect::to(&format!("/directory/{username}/")).into_response(),
            Err(_) => unavailable(&state),
        });
    }
    let after = query
        .after
        .map(|after| bbsbox_identity::normalize_username(&after))
        .transpose()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let rows = state
        .read(move |_, path| {
            bbsbox_identity::directory_usernames(&open_reader(path)?, after.as_deref())
        })
        .await?;
    let mut content = String::from(
        "<p><a href=\"/account/profile/\">Edit your profile</a></p><form action=\"/directory/\" method=\"get\"><label>Username <input name=\"username\" maxlength=\"32\" required></label><button type=\"submit\">Find account</button></form>",
    );
    content.push_str("<ul>");
    for username in &rows {
        let _ = write!(
            content,
            "<li><a href=\"/directory/{username}/\">{username}</a></li>"
        );
    }
    content.push_str("</ul>");
    if let Some(last) = rows.last().filter(|_| rows.len() == 50) {
        let _ = write!(
            content,
            "<p><a href=\"?after={last}\">Next accounts</a></p>"
        );
    }
    Ok(page(&state, StatusCode::OK, "Directory", &content))
}

async fn profile(
    State(state): State<Sessions>,
    Path(username): Path<String>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let Some((account, _)) = form_access(&state, &headers).await else {
        return Ok(Redirect::to("/login/").into_response());
    };
    let result = state
        .read(move |_, path| Ok(profiles::profile(&open_reader(path)?, &username)))
        .await?;
    let profile = match result {
        Ok(profile) => profile,
        Err(Error::Identity(
            bbsbox_identity::Error::AccountMissing | bbsbox_identity::Error::InvalidUsername,
        )) => return Ok(unavailable(&state)),
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };
    let mut content = String::from("<p><a href=\"/directory/\">Directory</a></p>");
    if account.username == profile.username {
        content.push_str("<p><a href=\"/account/profile/\">Edit your profile</a></p>");
    }
    let fields = profile.fields;
    for (label, value) in [
        ("Bio", fields.bio),
        ("Message", fields.message),
        ("Email", fields.email),
        ("Phone", fields.phone),
        ("Contact details", fields.contact),
    ] {
        if let Some(value) = value {
            let _ = write!(
                content,
                "<section><h2>{label}</h2><p class=\"welcome\">{}</p></section>",
                escape(&value)
            );
        }
    }
    Ok(page(&state, StatusCode::OK, &profile.username, &content))
}

async fn editor(State(state): State<Sessions>, headers: HeaderMap) -> Result<Response, StatusCode> {
    let Some((account, csrf)) = form_access(&state, &headers).await else {
        return Ok(Redirect::to("/login/").into_response());
    };
    let editor = state
        .read(move |_, path| Ok(profiles::editor(&open_reader(path)?, account.id)))
        .await?
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let csrf = csrf.as_str();
    let mut content = format!(
        "<p><a href=\"/directory/{}/\">View profile</a></p><form action=\"/account/profile/\" method=\"post\"><input type=\"hidden\" name=\"csrf\" value=\"{csrf}\"><input type=\"hidden\" name=\"revision\" value=\"{}\">",
        account.username, editor.revision
    );
    let fields = editor.preferences;
    checkbox(&mut content, "public", "Public profile", fields.public);
    content.push_str("<p>Choose which fields to show on your public profile.</p>");
    for (key, label, field, limit, multiline) in [
        ("bio", "Bio", fields.bio, 4096, true),
        ("message", "Short message", fields.message, 280, false),
        ("email", "Email", fields.email, 254, false),
        ("phone", "Phone", fields.phone, 64, false),
        ("contact", "Contact details", fields.contact, 512, true),
    ] {
        let value = escape(&field.text);
        let _ = write!(content, "<label>{label} ");
        if multiline {
            let _ = write!(
                content,
                "<textarea name=\"{key}\" maxlength=\"{limit}\" rows=\"4\">\n{value}</textarea>"
            );
        } else {
            let _ = write!(
                content,
                "<input name=\"{key}\" maxlength=\"{limit}\" value=\"{value}\">"
            );
        }
        content.push_str("</label>");
        checkbox(
            &mut content,
            &format!("show_{key}"),
            &format!("Show {}", label.to_lowercase()),
            field.publish,
        );
    }
    content.push_str("<button type=\"submit\">Save profile</button></form>");
    Ok(page(&state, StatusCode::OK, "Edit your profile", &content))
}

fn checkbox(content: &mut String, key: &str, label: &str, value: bool) {
    let checked = if value { " checked" } else { "" };
    let _ = write!(
        content,
        "<label class=\"choice\"><input type=\"checkbox\" name=\"{key}\" value=\"on\"{checked}> {label}</label>"
    );
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
    #[serde(deserialize_with = "password")]
    csrf: Zeroizing<String>,
    revision: i64,
    public: Option<String>,
    bio: String,
    message: String,
    email: String,
    phone: String,
    contact: String,
    show_bio: Option<String>,
    show_message: Option<String>,
    show_email: Option<String>,
    show_phone: Option<String>,
    show_contact: Option<String>,
}

fn field(text: String, choice: Option<String>) -> Field {
    Field {
        text: text.replace("\r\n", "\n"),
        publish: choice.as_deref() == Some("on"),
    }
}

async fn save(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    headers: HeaderMap,
    payload: Result<Form<Change>, FormRejection>,
) -> Result<Response, StatusCode> {
    check_origin(&headers, &origin)?;
    let session = token(&headers)?;
    let Form(change) = payload.map_err(|error| error.status())?;
    let value = Preferences {
        public: change.public.as_deref() == Some("on"),
        bio: field(change.bio, change.show_bio),
        message: field(change.message, change.show_message),
        email: field(change.email, change.show_email),
        phone: field(change.phone, change.show_phone),
        contact: field(change.contact, change.show_contact),
    };
    let status = state
        .write(move |auth, database| {
            database.write(Class::Control, 64 * 1024, |tx| {
                let actor = auth.authorize_mutation(tx, &session, &change.csrf, &origin.0)?;
                let account = auth.mutation_account(tx, &actor)?;
                Ok(
                    match profiles::save(tx, account.id, change.revision, &value) {
                        Ok(()) => Ok(account.username),
                        Err(Error::Conflict) => Err(StatusCode::CONFLICT),
                        Err(Error::InvalidField) => Err(StatusCode::UNPROCESSABLE_ENTITY),
                        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
                    },
                )
            })
        })
        .await?;
    Ok(match status {
        Ok(username) => Redirect::to(&format!("/directory/{username}/")).into_response(),
        Err(StatusCode::CONFLICT) => page(
            &state,
            StatusCode::CONFLICT,
            "Profile changed",
            "<p><a href=\"/account/profile/\">Review the current profile</a></p>",
        ),
        Err(StatusCode::UNPROCESSABLE_ENTITY) => page(
            &state,
            StatusCode::UNPROCESSABLE_ENTITY,
            "Check the field lengths",
            "<p>Go back to revise your entries.</p>",
        ),
        Err(status) => status.into_response(),
    })
}
