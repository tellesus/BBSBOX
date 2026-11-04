// SPDX-License-Identifier: AGPL-3.0-or-later
//! Local account creation for an authenticated SysOp.
use super::{RequestOrigin, Sessions, check_origin, html, password, sysop_csrf, template, token};
use axum::{
    Extension, Form, Router,
    extract::{State, rejection::FormRejection},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use bbsbox_identity::{Error, Role, normalize_username};
use bbsbox_storage::disk::Class;
use serde::Deserialize;
use zeroize::Zeroizing;

const TEMPLATE: &str = include_str!("../../../../../web/shell/compiled/accounts.html");

pub(super) fn routes() -> Router<Sessions> {
    Router::new().route("/settings/accounts/", get(editor).post(create))
}

fn render(state: &Sessions, csrf: &str, status: StatusCode, message: &str) -> Response {
    html(
        status,
        template(
            state,
            TEMPLATE,
            &[("ACCOUNT_STATUS", message), ("CSRF", csrf)],
        ),
    )
}

async fn editor(State(state): State<Sessions>, headers: HeaderMap) -> Result<Response, StatusCode> {
    match sysop_csrf(&state, &headers).await {
        Ok(csrf) => Ok(render(&state, &csrf, StatusCode::OK, "")),
        Err(StatusCode::UNAUTHORIZED) => Ok(Redirect::to("/login/").into_response()),
        Err(status) => Err(status),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum AccountRole {
    Member,
    Sysop,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Create {
    username: String,
    #[serde(deserialize_with = "password")]
    password: Zeroizing<String>,
    #[serde(deserialize_with = "password")]
    csrf: Zeroizing<String>,
    role: AccountRole,
}

async fn create(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    headers: HeaderMap,
    payload: Result<Form<Create>, FormRejection>,
) -> Result<Response, StatusCode> {
    check_origin(&headers, &origin)?;
    let csrf = sysop_csrf(&state, &headers).await?;
    let Form(change) = payload.map_err(|error| error.status())?;
    let session = token(&headers)?;
    let Ok(username) = normalize_username(&change.username) else {
        return Ok(render(
            &state,
            &csrf,
            StatusCode::UNPROCESSABLE_ENTITY,
            "Use 1–32 letters, digits, underscores or hyphens for the username.",
        ));
    };
    let role = match change.role {
        AccountRole::Member => Role::Member,
        AccountRole::Sysop => Role::Sysop,
    };
    let prepared = state
        .read(
            move |auth, _| match auth.passwords().prepare(change.password) {
                Ok(password) => Ok(Some(password)),
                Err(Error::WeakPassword) => Ok(None),
                Err(error) => Err(error),
            },
        )
        .await?;
    let Some(prepared) = prepared else {
        return Ok(render(
            &state,
            &csrf,
            StatusCode::UNPROCESSABLE_ENTITY,
            "Use at least 15 characters and at most 1024 bytes for the password.",
        ));
    };
    let created = state
        .write(move |auth, database| {
            let result = database.write(Class::Control, 64 * 1024, |tx| {
                let actor = auth.authorize_mutation(tx, &session, &change.csrf, &origin.0)?;
                auth.create_account(tx, &actor, &username, prepared, role)
            });
            match result {
                Ok(_) => Ok(true),
                Err(Error::UsernameTaken) => Ok(false),
                Err(error) => Err(error),
            }
        })
        .await?;
    if created {
        Ok(render(
            &state,
            &csrf,
            StatusCode::CREATED,
            "Account created.",
        ))
    } else {
        Ok(render(
            &state,
            &csrf,
            StatusCode::CONFLICT,
            "That username is already in use.",
        ))
    }
}
