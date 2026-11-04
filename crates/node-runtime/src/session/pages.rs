// SPDX-License-Identifier: AGPL-3.0-or-later
//! Local server forms built from prerendered SvelteKit templates.
use super::{
    Credentials, RequestOrigin, Sessions, change_session, check_origin, cookie_header,
    current_account, password, role_name, sign_in, token, with_cookies,
};
use axum::{
    Extension, Form, Router,
    extract::{State, rejection::FormRejection},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use bbsbox_identity::{
    Role,
    session::{Login, csrf_from_cookie_header},
};
use serde::Deserialize;
use zeroize::Zeroizing;

const HOME: &str = include_str!("../../../../web/shell/compiled/home.html");
const SIGN_IN: &str = include_str!("../../../../web/shell/compiled/login.html");
const ACCOUNT: &str = include_str!("../../../../web/shell/compiled/account.html");
const STYLE: &str = include_str!("../../../../web/shell/static/shell.css");
mod theme;

pub(super) fn routes() -> Router<Sessions> {
    Router::new()
        .route("/", get(home))
        .route("/login/", get(login_page).post(login_form))
        .route("/account/", get(account))
        .route("/account/sign-out/", post(sign_out_form))
        .merge(theme::routes())
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
    html(
        StatusCode::OK,
        HOME.replace("__BBSBOX_NODE_NAME__", &escape(&state.node_name)),
    )
}

fn render_login(node: &str, status: StatusCode, message: &str) -> Response {
    let body = SIGN_IN
        .replace("__BBSBOX_LOGIN_MESSAGE__", message)
        .replace("__BBSBOX_NODE_NAME__", &escape(node));
    html(status, body)
}

async fn login_page(State(state): State<Sessions>) -> Response {
    render_login(&state.node_name, StatusCode::OK, "")
}

async fn login_form(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    headers: HeaderMap,
    payload: Result<Form<Credentials>, FormRejection>,
) -> Result<Response, StatusCode> {
    check_origin(&headers, &origin)?;
    let name = state.node_name.clone();
    let Form(credentials) = match payload {
        Ok(payload) => payload,
        Err(error) => {
            return Ok(render_login(
                &name,
                error.status(),
                "Enter your username and password.",
            ));
        }
    };
    let login = match sign_in(state, origin, credentials).await {
        Ok(login) => login,
        Err(status) => {
            let message = match status {
                StatusCode::UNAUTHORIZED => "Username or password is incorrect.",
                StatusCode::TOO_MANY_REQUESTS => "Too many attempts. Try again in a minute.",
                _ => "Sign-in is unavailable. Try again shortly.",
            };
            return Ok(render_login(&name, status, message));
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
    let body = ACCOUNT
        .replace("__BBSBOX_CSRF__", csrf)
        .replace("__BBSBOX_ROLE__", role_name(account.role))
        .replace(
            "__BBSBOX_SYSOP_LINK__",
            if account.role == Role::Sysop {
                "<p><a href=\"/settings/theme/\">Theme settings</a></p>"
            } else {
                ""
            },
        )
        .replace("__BBSBOX_USERNAME__", &escape(&account.username));
    Ok(html(StatusCode::OK, body))
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
