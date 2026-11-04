// SPDX-License-Identifier: AGPL-3.0-or-later
//! Same-origin HTTP transport for local account sessions.
use axum::{
    Extension, Json, Router,
    extract::{DefaultBodyLimit, State, rejection::JsonRejection},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use bbsbox_identity::{
    Account, Auth, Error, Role,
    session::{Login, session_from_cookie_header},
};
use bbsbox_storage::{
    Database,
    clock::SystemClock,
    disk::Class,
    writer::{Handle, Priority},
};
use serde::Deserialize;
use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
};
use tokio::sync::Semaphore;
use zeroize::Zeroizing;

mod pages;

#[derive(Clone)]
pub(crate) struct RequestOrigin(pub String);

#[derive(Clone)]
struct Sessions {
    auth: Arc<Auth>,
    database: PathBuf,
    writer: Handle,
    workers: Arc<Semaphore>,
    node_name: Arc<str>,
    theme: Arc<RwLock<bbsbox_settings::Theme>>,
}

pub(super) fn router(config: &crate::Config, writer: Handle) -> crate::Result<Router> {
    let origins = std::iter::once(&config.origin).chain(&config.additional_origins);
    let auth = Auth::with_origins(
        origins.map(String::as_str),
        Arc::new(SystemClock::default()),
        Default::default(),
    )?;
    let reader = bbsbox_storage::open_reader(&config.database)?;
    let theme = bbsbox_settings::theme(&reader)?;
    let state = Sessions {
        auth: Arc::new(auth),
        database: config.database.clone(),
        writer,
        workers: Arc::new(Semaphore::new(2)),
        node_name: Arc::from(config.name.as_str()),
        theme: Arc::new(RwLock::new(theme)),
    };
    Ok(Router::new()
        .route(
            "/api/v1/session",
            get(current).post(login).patch(refresh).delete(logout),
        )
        .merge(pages::routes())
        .layer(DefaultBodyLimit::max(8192))
        .with_state(state))
}

fn failure(error: Error) -> StatusCode {
    use bbsbox_storage::Error as Storage;
    match error {
        Error::InvalidUsername
        | Error::InvalidCredentials
        | Error::InvalidSession
        | Error::CredentialsChanged => StatusCode::UNAUTHORIZED,
        Error::CsrfDenied | Error::Forbidden => StatusCode::FORBIDDEN,
        Error::RateLimited => StatusCode::TOO_MANY_REQUESTS,
        Error::Busy
        | Error::Storage(
            Storage::QueueFull
            | Storage::MaintenancePaused
            | Storage::WriterClosed
            | Storage::DiskPressure { .. },
        ) => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

impl Sessions {
    async fn read<T: Send + 'static>(
        &self,
        work: impl FnOnce(&Auth, PathBuf) -> bbsbox_identity::Result<T> + Send + 'static,
    ) -> Result<T, StatusCode> {
        let permit = self
            .workers
            .clone()
            .try_acquire_owned()
            .map_err(|_| failure(Error::Busy))?;
        let database = self.database.clone();
        let auth = self.auth.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            work(&auth, database)
        })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map_err(failure)
    }

    async fn write<T: Send + 'static>(
        &self,
        work: impl FnOnce(&Auth, &mut Database) -> bbsbox_identity::Result<T> + Send + 'static,
    ) -> Result<T, StatusCode> {
        let auth = self.auth.clone();
        self.writer
            .submit_service(Priority::Interactive, move |database| work(&auth, database))
            .map_err(|error| failure(error.into()))?
            .wait()
            .await
            .map_err(failure)
    }
}

fn one_header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let mut values = headers.get_all(name).iter();
    let value = values.next()?.to_str().ok()?;
    values.next().is_none().then_some(value)
}

fn check_origin(headers: &HeaderMap, origin: &RequestOrigin) -> Result<(), StatusCode> {
    if one_header(headers, "origin") != Some(origin.0.as_str()) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

fn cookie_header(headers: &HeaderMap) -> Result<Zeroizing<String>, StatusCode> {
    let mut cookies = Zeroizing::new(String::new());
    for value in headers.get_all(header::COOKIE) {
        let value = value.to_str().map_err(|_| StatusCode::UNAUTHORIZED)?;
        if cookies.len() + value.len() + 1 > 8192 {
            return Err(StatusCode::UNAUTHORIZED);
        }
        cookies.push_str(value);
        cookies.push(';');
    }
    Ok(cookies)
}

fn token(headers: &HeaderMap) -> Result<Zeroizing<String>, StatusCode> {
    let cookies = cookie_header(headers)?;
    let token = session_from_cookie_header(&cookies).map_err(failure)?;
    Ok(Zeroizing::new(token.to_owned()))
}

fn role_name(role: Role) -> &'static str {
    match role {
        Role::Member => "member",
        Role::Sysop => "sysop",
    }
}

fn public_account(account: Account) -> Json<serde_json::Value> {
    let role = role_name(account.role);
    Json(
        serde_json::json!({"format":"bbsbox.local-session/v1","username":account.username,"role":role}),
    )
}

fn with_cookies(response: impl IntoResponse, cookies: [&str; 2]) -> Response {
    let mut response = response.into_response();
    for cookie in cookies {
        let mut value = HeaderValue::from_str(cookie).expect("ASCII session cookie");
        value.set_sensitive(true);
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    response
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Credentials {
    username: String,
    #[serde(deserialize_with = "password")]
    password: Zeroizing<String>,
}

fn password<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Zeroizing<String>, D::Error> {
    String::deserialize(deserializer).map(Zeroizing::new)
}

async fn login(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    headers: HeaderMap,
    payload: Result<Json<Credentials>, JsonRejection>,
) -> Result<Response, StatusCode> {
    check_origin(&headers, &origin)?;
    let Json(credentials) = payload.map_err(|error| error.status())?;
    let login = sign_in(state, origin, credentials).await?;
    let [session, csrf] = login.cookie_headers().map(Zeroizing::new);
    Ok(with_cookies(
        public_account(login.account),
        [&session, &csrf],
    ))
}

async fn sign_in(
    state: Sessions,
    origin: RequestOrigin,
    credentials: Credentials,
) -> Result<Login, StatusCode> {
    let proof = state
        .read(move |auth, database| {
            let reader = bbsbox_storage::open_reader(database)?;
            auth.verify_password(
                &reader,
                &credentials.username,
                credentials.password,
                &origin.0,
            )
        })
        .await?;
    state
        .write(move |auth, database| {
            database.write(Class::Control, 64 * 1024, |tx| {
                auth.complete_login(tx, proof)
            })
        })
        .await
}

async fn current(
    State(state): State<Sessions>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    current_account(&state, &headers).await.map(public_account)
}

async fn current_account(state: &Sessions, headers: &HeaderMap) -> Result<Account, StatusCode> {
    let token = token(headers)?;
    state
        .read(move |auth, database| {
            let reader = bbsbox_storage::open_reader(database)?;
            auth.authenticate(&reader, &token)
        })
        .await
}

async fn refresh(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    headers: HeaderMap,
) -> Result<StatusCode, StatusCode> {
    change_api_session(state, origin, headers, false).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn logout(
    State(state): State<Sessions>,
    Extension(origin): Extension<RequestOrigin>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    change_api_session(state, origin, headers, true).await?;
    Ok(with_cookies(
        StatusCode::NO_CONTENT,
        Login::clear_cookie_headers(),
    ))
}

async fn change_api_session(
    state: Sessions,
    origin: RequestOrigin,
    headers: HeaderMap,
    logout: bool,
) -> Result<(), StatusCode> {
    check_origin(&headers, &origin)?;
    let token = token(&headers)?;
    let csrf = one_header(&headers, "x-bbsbox-csrf")
        .filter(|value| value.len() == 64)
        .ok_or(StatusCode::FORBIDDEN)?;
    let csrf = Zeroizing::new(csrf.to_owned());
    change_session(state, origin, token, csrf, logout).await
}

async fn change_session(
    state: Sessions,
    origin: RequestOrigin,
    token: Zeroizing<String>,
    csrf: Zeroizing<String>,
    logout: bool,
) -> Result<(), StatusCode> {
    state
        .write(move |auth, database| {
            database.write(Class::Control, 64 * 1024, |tx| {
                let actor = auth.authorize_mutation(tx, &token, &csrf, &origin.0)?;
                if logout {
                    auth.logout(tx, &actor)
                } else {
                    auth.refresh_session(tx, &actor)
                }
            })
        })
        .await
}
