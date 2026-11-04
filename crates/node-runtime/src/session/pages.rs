// SPDX-License-Identifier: AGPL-3.0-or-later
//! Local server forms built from prerendered SvelteKit templates.
use super::{
    Credentials, RequestOrigin, Sessions, change_session, check_origin, cookie_header,
    current_account, password, role_name, sign_in, token, with_cookies,
};
use axum::{
    Extension, Form, Router,
    extract::{OriginalUri, State, rejection::FormRejection},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use bbsbox_event_codec::EventId;
use bbsbox_identity::Account;
use bbsbox_identity::{
    Role,
    session::{Login, csrf_from_cookie_header},
};
use rand_core::{OsRng, RngCore};
use serde::Deserialize;
use std::fmt::Write;
use zeroize::Zeroizing;

const HOME: &str = include_str!("../../../../web/shell/compiled/home.html");
const SIGN_IN: &str = include_str!("../../../../web/shell/compiled/login.html");
const ACCOUNT: &str = include_str!("../../../../web/shell/compiled/account.html");
const STYLE: &str = include_str!("../../../../web/shell/static/shell.css");
const UNAVAILABLE: &str = include_str!("../../../../web/shell/compiled/unavailable.html");
const SHELL_STATE: &str = include_str!("../../../../web/shell/static/shell-state.js");
mod accounts;
mod discussions;
mod mail;
mod profiles;
mod site;
mod theme;

pub(super) fn routes() -> Router<Sessions> {
    Router::new()
        .route("/", get(home))
        .route("/login/", get(login_page).post(login_form))
        .route("/account/", get(account))
        .route("/account/sign-out/", post(sign_out_form))
        .route(
            "/assets/shell-state.js",
            get(|| async { crate::asset(SHELL_STATE, "text/javascript; charset=utf-8") }),
        )
        .merge(accounts::routes())
        .merge(discussions::routes())
        .merge(mail::routes())
        .merge(profiles::routes())
        .merge(site::routes())
        .merge(theme::routes())
        .fallback(fallback)
}

fn template(state: &Sessions, mut source: &str, slots: &[(&str, &str)]) -> String {
    let menu = state
        .site
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .menu();
    let mut body = String::with_capacity(source.len());
    while let Some((before, suffix)) = source.split_once("__BBSBOX_") {
        let (slot, rest) = suffix.split_once("__").expect("verified template slot");
        body.push_str(before);
        if slot == "NAV" {
            for item in menu.split(',') {
                body.push_str(match item {
                    "home" => "<a href=\"/\">Home</a>",
                    "account" => "<a href=\"/account/\">Account</a>",
                    "status" => "<a href=\"/status/\">Status</a>",
                    _ => unreachable!("validated menu"),
                });
            }
            body.push_str("<a href=\"/discussions/\">Discussions</a>");
            body.push_str("<a href=\"/mail/\">Mail</a>");
            body.push_str("<a href=\"/directory/\">Directory</a>");
        } else {
            body.push_str(
                slots
                    .iter()
                    .find(|(key, _)| *key == slot)
                    .expect("known template slot")
                    .1,
            );
        }
        source = rest;
    }
    body.push_str(source);
    body
}

fn event_key(id: EventId) -> String {
    hex::encode(id.bytes())
}

fn event_id(value: &str) -> Result<EventId, StatusCode> {
    let mut bytes = [0; 32];
    if value.len() != 64
        || value
            .bytes()
            .any(|byte| !(byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
        || hex::decode_to_slice(value, &mut bytes).is_err()
    {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(EventId::from_bytes(bytes))
}

async fn form_access(
    state: &Sessions,
    headers: &HeaderMap,
) -> Option<(Account, zeroize::Zeroizing<String>)> {
    let account = current_account(state, headers).await.ok()?;
    let cookies = cookie_header(headers).ok()?;
    let csrf = csrf_from_cookie_header(&cookies).ok()?;
    Some((account, zeroize::Zeroizing::new(csrf.to_owned())))
}

fn form(content: &mut String, action: &str, csrf: &str, label: &str, fields: &str) {
    let mut request = [0; 16];
    OsRng.fill_bytes(&mut request);
    let request = hex::encode(request);
    let _ = write!(
        content,
        "<h2>{label}</h2><form action=\"{action}\" method=\"post\"><input type=\"hidden\" name=\"csrf\" value=\"{csrf}\"><input type=\"hidden\" name=\"request\" value=\"{request}\">{fields}<button type=\"submit\">{label}</button></form>"
    );
}

fn html(status: StatusCode, body: String) -> Response {
    let mut response = (status, Html(body)).into_response();
    // Native POST forms include the exact same-origin Origin header under this policy.
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("same-origin"),
    );
    response
}

async fn fallback(
    State(state): State<Sessions>,
    OriginalUri(uri): OriginalUri,
    method: Method,
) -> Response {
    let path = uri.path();
    if path.starts_with("/api/")
        || path.starts_with("/assets/")
        || !matches!(method, Method::GET | Method::HEAD)
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    let section = path.split('/').nth(1).unwrap_or_default();
    let (title, message) = match section {
        "people" | "discussions" | "events" | "mail" | "games" => (
            "Section unavailable",
            "This section is unavailable on this node.",
        ),
        _ => (
            "Page not found",
            "Check the address or return to the node home page.",
        ),
    };
    html(
        StatusCode::NOT_FOUND,
        template(
            &state,
            UNAVAILABLE,
            &[("PAGE_TITLE", title), ("PAGE_MESSAGE", message)],
        ),
    )
}

fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            character => escaped.push(character),
        }
    }
    escaped
}

async fn home(State(state): State<Sessions>) -> Response {
    let welcome = site::welcome(&state);
    html(
        StatusCode::OK,
        template(
            &state,
            HOME,
            &[
                ("NODE_NAME", &escape(&state.node_name)),
                ("WELCOME", &welcome),
            ],
        ),
    )
}

fn render_login(state: &Sessions, status: StatusCode, message: &str) -> Response {
    let body = template(
        state,
        SIGN_IN,
        &[
            ("LOGIN_MESSAGE", message),
            ("NODE_NAME", &escape(&state.node_name)),
        ],
    );
    html(status, body)
}

async fn login_page(State(state): State<Sessions>) -> Response {
    render_login(&state, StatusCode::OK, "")
}

async fn login_form(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    headers: HeaderMap,
    payload: Result<Form<Credentials>, FormRejection>,
) -> Result<Response, StatusCode> {
    check_origin(&headers, &origin)?;
    let Form(credentials) = match payload {
        Ok(payload) => payload,
        Err(error) => {
            return Ok(render_login(
                &state,
                error.status(),
                "Enter your username and password.",
            ));
        }
    };
    let login = match sign_in(state.clone(), origin, credentials).await {
        Ok(login) => login,
        Err(status) => {
            let message = match status {
                StatusCode::UNAUTHORIZED => "Username or password is incorrect.",
                StatusCode::TOO_MANY_REQUESTS => "Too many attempts. Try again in a minute.",
                _ => "Sign-in is unavailable. Try again shortly.",
            };
            return Ok(render_login(&state, status, message));
        }
    };
    let [session, csrf] = login.cookie_headers().map(Zeroizing::new);
    Ok(with_cookies(Redirect::to("/account/"), [&session, &csrf]))
}

async fn account(
    State(state): State<Sessions>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let account = match current_account(&state, &headers).await {
        Ok(account) => account,
        Err(StatusCode::UNAUTHORIZED) => return Ok(Redirect::to("/login/").into_response()),
        Err(status) => return Err(status),
    };
    let cookies = cookie_header(&headers)?;
    let Ok(csrf) = csrf_from_cookie_header(&cookies) else {
        return Ok(Redirect::to("/login/").into_response());
    };
    let sysop_links = if account.role == Role::Sysop {
        "<p><a href=\"/settings/theme/\">Theme settings</a> · <a href=\"/settings/site/\">Site settings</a> · <a href=\"/settings/accounts/\">Create account</a></p>"
    } else {
        ""
    };
    let body = template(
        &state,
        ACCOUNT,
        &[
            ("CSRF", csrf),
            ("ROLE", role_name(account.role)),
            ("SYSOP_LINK", sysop_links),
            ("USERNAME", &escape(&account.username)),
        ],
    );
    Ok(html(StatusCode::OK, body))
}

async fn sysop_csrf(
    state: &Sessions,
    headers: &HeaderMap,
) -> Result<Zeroizing<String>, StatusCode> {
    if current_account(state, headers).await?.role != Role::Sysop {
        return Err(StatusCode::FORBIDDEN);
    }
    let cookies = cookie_header(headers)?;
    let csrf = csrf_from_cookie_header(&cookies).map_err(|_| StatusCode::UNAUTHORIZED)?;
    Ok(Zeroizing::new(csrf.to_owned()))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SignOut {
    #[serde(deserialize_with = "password")]
    csrf: Zeroizing<String>,
}

async fn sign_out_form(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    headers: HeaderMap,
    payload: Result<Form<SignOut>, FormRejection>,
) -> Result<Response, StatusCode> {
    check_origin(&headers, &origin)?;
    let Form(form) = payload.map_err(|error| error.status())?;
    let session = match token(&headers) {
        Ok(session) => session,
        Err(StatusCode::UNAUTHORIZED) => return Ok(Redirect::to("/login/").into_response()),
        Err(status) => return Err(status),
    };
    match change_session(state, origin, session, form.csrf, true).await {
        Ok(()) => Ok(with_cookies(
            Redirect::to("/"),
            Login::clear_cookie_headers(),
        )),
        Err(StatusCode::UNAUTHORIZED) => Ok(Redirect::to("/login/").into_response()),
        Err(status) => Err(status),
    }
}
