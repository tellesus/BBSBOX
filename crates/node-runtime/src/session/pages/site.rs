// SPDX-License-Identifier: AGPL-3.0-or-later
//! SysOp editor for the local landing page and menu order.
use super::{
    RequestOrigin, Sessions, check_origin, escape, html, password, sysop_csrf, template, token,
};
use axum::{
    Extension, Form, Router,
    extract::{State, rejection::FormRejection},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use bbsbox_identity::Error;
use bbsbox_settings::{
    Error as SettingsError,
    site::{MENUS, Site},
};
use bbsbox_storage::disk::Class;
use serde::Deserialize;
use std::fmt::Write;
use zeroize::Zeroizing;

const TEMPLATE: &str = include_str!("../../../../../web/shell/compiled/site.html");

pub(super) fn routes() -> Router<Sessions> {
    Router::new().route("/settings/site/", get(editor).post(save))
}

fn current(state: &Sessions) -> Site {
    state
        .site
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

pub(super) fn welcome(state: &Sessions) -> String {
    let site = current(state);
    let mut content = String::new();
    if !site.welcome.is_empty() {
        let _ = write!(
            content,
            "<p class=\"welcome\">{}</p>",
            escape(&site.welcome)
        );
    }
    if !site.artwork.is_empty() {
        let _ = write!(
            content,
            "<pre class=\"welcome-art\" aria-hidden=\"true\">\n{}</pre>",
            escape(&site.artwork)
        );
    }
    content
}

fn render(state: &Sessions, csrf: &str, status: StatusCode, message: &str) -> Response {
    let site = current(state);
    let mut options = String::new();
    for (value, label) in MENUS {
        let selected = if site.menu() == value {
            " selected"
        } else {
            ""
        };
        let _ = write!(
            options,
            "<option value=\"{value}\"{selected}>{label}</option>"
        );
    }
    html(
        status,
        template(
            state,
            TEMPLATE,
            &[
                ("SITE_STATUS", message),
                ("SITE_REVISION", &site.revision.to_string()),
                ("CSRF", csrf),
                ("MESSAGE", &format!("\n{}", escape(&site.welcome))),
                ("ART", &format!("\n{}", escape(&site.artwork))),
                ("MENU_OPTIONS", &options),
            ],
        ),
    )
}

async fn editor(State(state): State<Sessions>, headers: HeaderMap) -> Result<Response, StatusCode> {
    match sysop_csrf(&state, &headers).await {
        Ok(csrf) => {
            let site = current(&state);
            let message = if site.welcome.is_empty() && site.artwork.is_empty() {
                "No welcome material is configured."
            } else {
                ""
            };
            Ok(render(&state, &csrf, StatusCode::OK, message))
        }
        Err(StatusCode::UNAUTHORIZED) => Ok(Redirect::to("/login/").into_response()),
        Err(status) => Err(status),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
    revision: i64,
    #[serde(deserialize_with = "password")]
    csrf: Zeroizing<String>,
    welcome: String,
    artwork: String,
    menu: String,
}

async fn save(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    headers: HeaderMap,
    payload: Result<Form<Change>, FormRejection>,
) -> Result<Response, StatusCode> {
    check_origin(&headers, &origin)?;
    let csrf = sysop_csrf(&state, &headers).await?;
    let Form(change) = payload.map_err(|error| error.status())?;
    let Some(proposed) = Site::new(
        change.revision,
        &change.welcome,
        &change.artwork,
        &change.menu,
    ) else {
        return Ok(render(
            &state,
            &csrf,
            StatusCode::UNPROCESSABLE_ENTITY,
            "Check the message length, text art and menu order. Text art allows up to 16 lines of 80 characters.",
        ));
    };
    let session = token(&headers)?;
    let cache = state.site.clone();
    let saved = state
        .write(move |auth, database| {
            let result = database.write(
                Class::Control,
                64 * 1024,
                |tx| -> bbsbox_identity::Result<Option<Site>> {
                    let actor = auth.authorize_mutation(tx, &session, &change.csrf, &origin.0)?;
                    auth.require_sysop(tx, &actor)?;
                    match bbsbox_settings::site::apply(tx, proposed) {
                        Ok(site) => Ok(Some(site)),
                        Err(SettingsError::Conflict) => Ok(None),
                        Err(SettingsError::Sqlite(error)) => Err(error.into()),
                        Err(_) => Err(Error::InvalidConfiguration),
                    }
                },
            )?;
            if let Some(site) = result {
                *cache
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = site;
                Ok(true)
            } else {
                Ok(false)
            }
        })
        .await?;
    if saved {
        Ok(Redirect::to("/settings/site/").into_response())
    } else {
        Ok(render(
            &state,
            &csrf,
            StatusCode::CONFLICT,
            "Site settings changed. Review the current values and try again.",
        ))
    }
}
