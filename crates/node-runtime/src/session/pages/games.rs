// SPDX-License-Identifier: AGPL-3.0-or-later
//! Local account-side launch. The game origin receives only an expiring ticket.
use super::{RequestOrigin, Sessions, check_origin, form_access, html, password, template, token};
use axum::{
    Extension, Form, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use serde::Deserialize;
use zeroize::Zeroizing;

const TEMPLATE: &str = include_str!("../../../../../web/shell/compiled/games.html");

pub(super) fn routes() -> Router<Sessions> {
    Router::new().route("/games/", get(list).post(launch))
}

async fn list(State(state): State<Sessions>, headers: HeaderMap) -> Response {
    if state.game.is_none() {
        return html(
            StatusCode::NOT_FOUND,
            template(
                &state,
                TEMPLATE,
                &[("GAMES_CONTENT", "<p>Section unavailable</p>")],
            ),
        );
    }
    let Some((_, csrf)) = form_access(&state, &headers).await else {
        return Redirect::to("/login/").into_response();
    };
    let content = format!(
        "<h2>Dungeon demo</h2><p>Two local players take turns exploring.</p><form method=\"post\" action=\"/games/\"><input type=\"hidden\" name=\"csrf\" value=\"{}\"><button type=\"submit\">Open game</button></form>",
        csrf.as_str()
    );
    html(
        StatusCode::OK,
        template(&state, TEMPLATE, &[("GAMES_CONTENT", &content)]),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Launch {
    #[serde(deserialize_with = "password")]
    csrf: Zeroizing<String>,
}

async fn launch(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    headers: HeaderMap,
    Form(launch): Form<Launch>,
) -> Result<Response, StatusCode> {
    let game = state.game.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    check_origin(&headers, &origin)?;
    let session = token(&headers)?;
    let (account, actor) = state
        .read(move |auth, path| {
            let reader = bbsbox_storage::open_reader(path)?;
            let tx = reader.unchecked_transaction()?;
            let actor = auth.authorize_mutation(&tx, &session, &launch.csrf, &origin.0)?;
            Ok((auth.mutation_account(&tx, &actor)?.id, actor))
        })
        .await?;
    let secret = game.ticket(account, state.auth.clone(), actor)?;
    Ok(Redirect::to(&format!("{}/#ticket={}", game.origin, secret.expose())).into_response())
}
