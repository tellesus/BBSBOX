// SPDX-License-Identifier: AGPL-3.0-or-later
//! Validated node theme preview and revision-checked publication.
use super::{
    RequestOrigin, STYLE, Sessions, check_origin, cookie_header, current_account, html, password,
    token,
};
use axum::{
    Extension, Form, Router,
    extract::{Path, Query, State, rejection::FormRejection},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use bbsbox_identity::{Error, Role, session::csrf_from_cookie_header};
use bbsbox_settings::{Error as ThemeError, Palette, Theme};
use bbsbox_storage::disk::Class;
use serde::Deserialize;
use zeroize::Zeroizing;

const TEMPLATE: &str = include_str!("../../../../../web/shell/compiled/theme.html");

pub(super) fn routes() -> Router<Sessions> {
    Router::new()
        .route("/settings/theme/", get(editor))
        .route("/settings/theme/{action}/", post(submit))
        .route("/assets/shell.css", get(stylesheet))
}

fn current_theme(state: &Sessions) -> Theme {
    state
        .theme
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StyleQuery {
    preview: Option<String>,
}

async fn stylesheet(
    State(state): State<Sessions>,
    Query(query): Query<StyleQuery>,
) -> Result<Response, StatusCode> {
    let palette = match query.preview {
        None => current_theme(&state).palette,
        Some(preview) => {
            if preview.len() != 31 {
                return Err(StatusCode::BAD_REQUEST);
            }
            let colors = preview.split(',').collect::<Vec<_>>();
            Palette::new(colors.try_into().map_err(|_| StatusCode::BAD_REQUEST)?)
                .map_err(|_| StatusCode::BAD_REQUEST)?
        }
    };
    let [background, text, accent, secondary] = palette.colors();
    let scheme = palette.color_scheme();
    let css = format!(
        "{STYLE}\n:root {{ color-scheme: {scheme}; --background: {background}; --text: {text}; --accent: {accent}; --secondary-accent: {secondary}; }}\n"
    );
    Ok(([(header::CONTENT_TYPE, "text/css; charset=utf-8")], css).into_response())
}

async fn editor(State(state): State<Sessions>, headers: HeaderMap) -> Result<Response, StatusCode> {
    let csrf = match sysop_csrf(&state, &headers).await {
        Ok(csrf) => csrf,
        Err(StatusCode::UNAUTHORIZED) => return Ok(Redirect::to("/login/").into_response()),
        Err(status) => return Err(status),
    };
    let theme = current_theme(&state);
    Ok(render(
        &theme,
        &csrf,
        StatusCode::OK,
        "Current theme",
        false,
    ))
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

fn render(theme: &Theme, csrf: &str, status: StatusCode, message: &str, preview: bool) -> Response {
    let [background, text, accent, secondary] = theme.palette.colors();
    let mut body = TEMPLATE
        .replace("__BBSBOX_THEME_STATUS__", message)
        .replace("__BBSBOX_THEME_REVISION__", &theme.revision.to_string())
        .replace("__BBSBOX_CSRF__", csrf)
        .replace("__BBSBOX_BACKGROUND__", background)
        .replace("__BBSBOX_TEXT__", text)
        .replace("__BBSBOX_ACCENT__", accent)
        .replace("__BBSBOX_SECONDARY__", secondary);
    if preview {
        let query = theme
            .palette
            .colors()
            .map(|color| format!("%23{}", &color[1..]))
            .join(",");
        body = body.replace(
            "/assets/shell.css",
            &format!("/assets/shell.css?preview={query}"),
        );
    }
    html(status, body)
}

#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Action {
    Preview,
    Apply,
    Reset,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
    revision: i64,
    #[serde(deserialize_with = "password")]
    csrf: Zeroizing<String>,
    background: String,
    text: String,
    accent: String,
    secondary: String,
}

async fn submit(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    Path(action): Path<Action>,
    headers: HeaderMap,
    payload: Result<Form<Change>, FormRejection>,
) -> Result<Response, StatusCode> {
    check_origin(&headers, &origin)?;
    let Form(change) = payload.map_err(|error| error.status())?;
    let csrf = sysop_csrf(&state, &headers).await?;
    let active = current_theme(&state);
    let conflict = |state: &Sessions| {
        render(
            &current_theme(state),
            &csrf,
            StatusCode::CONFLICT,
            "The theme changed. Review the current colors and try again.",
            false,
        )
    };
    if change.revision != active.revision {
        return Ok(conflict(&state));
    }
    let palette = match action {
        Action::Reset => Palette::default(),
        _ => match Palette::new([
            &change.background,
            &change.text,
            &change.accent,
            &change.secondary,
        ]) {
            Ok(palette) => palette,
            Err(error) => {
                let message = match error {
                    ThemeError::LowContrast => {
                        "Choose text, link and focus colors with more contrast against the background."
                    }
                    _ => "Enter a valid six-digit color for each field.",
                };
                return Ok(render(
                    &active,
                    &csrf,
                    StatusCode::UNPROCESSABLE_ENTITY,
                    message,
                    false,
                ));
            }
        },
    };
    if action == Action::Preview {
        let theme = Theme {
            revision: active.revision,
            palette,
        };
        return Ok(render(
            &theme,
            &csrf,
            StatusCode::OK,
            "Preview. Apply to use these colors on the node.",
            true,
        ));
    }

    let token = token(&headers)?;
    let cache = state.theme.clone();
    let saved = state
        .write(move |auth, database| {
            let result = database.write(
                Class::Control,
                64 * 1024,
                |tx| -> bbsbox_identity::Result<Option<Theme>> {
                    let actor = auth.authorize_mutation(tx, &token, &change.csrf, &origin.0)?;
                    auth.require_sysop(tx, &actor)?;
                    match bbsbox_settings::apply(tx, change.revision, palette) {
                        Ok(theme) => Ok(Some(theme)),
                        Err(ThemeError::Conflict) => Ok(None),
                        Err(ThemeError::Sqlite(error)) => Err(error.into()),
                        Err(_) => Err(Error::InvalidConfiguration),
                    }
                },
            )?;
            if let Some(theme) = result {
                *cache
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = theme;
                Ok(true)
            } else {
                Ok(false)
            }
        })
        .await?;
    if saved {
        Ok(Redirect::to("/settings/theme/").into_response())
    } else {
        Ok(conflict(&state))
    }
}
