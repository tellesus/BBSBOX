// SPDX-License-Identifier: AGPL-3.0-or-later
mod event;
mod store;

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use ed25519_dalek::SigningKey;
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use store::{Message, Store};
use tower_http::{services::ServeDir, set_header::SetResponseHeaderLayer};

type Shared = Arc<Mutex<Store>>;
#[derive(Debug)]
struct ApiError(StatusCode, &'static str);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({ "error": self.1 }))).into_response()
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewMessage {
    request_id: String,
    label: String,
    text: String,
}
#[derive(Serialize)]
struct NodeInfo {
    name: &'static str,
    stage: &'static str,
    public_key: String,
    messages: usize,
    sqlite: &'static str,
}

async fn with_store<T: Send + 'static>(
    state: Shared,
    f: impl FnOnce(&mut Store) -> Result<T, String> + Send + 'static,
) -> Result<T, ApiError> {
    tokio::task::spawn_blocking(move || {
        let mut store = state.lock().map_err(|_| {
            ApiError(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Local storage is unavailable.",
            )
        })?;
        f(&mut store).map_err(|error| {
            eprintln!("storage check: {error}");
            ApiError(
                StatusCode::CONFLICT,
                "The local store could not accept or verify this request. Check the node terminal.",
            )
        })
    })
    .await
    .map_err(|_| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Local storage is unavailable.",
        )
    })?
}
async fn info(State(state): State<Shared>) -> Result<Json<NodeInfo>, ApiError> {
    with_store(state, |s| {
        Ok(NodeInfo {
            name: "BBSBOX / foundation",
            stage: "M00 experiment",
            public_key: s.public_key(),
            messages: s.messages()?.len(),
            sqlite: s.sqlite_version(),
        })
    })
    .await
    .map(Json)
}
async fn messages(State(state): State<Shared>) -> Result<Json<Vec<Message>>, ApiError> {
    with_store(state, |s| {
        let mut messages = s.messages()?;
        messages.reverse();
        messages.truncate(100);
        Ok(messages)
    })
    .await
    .map(Json)
}
fn check_origin(headers: &HeaderMap, uri: &axum::http::Uri) -> Result<(), ApiError> {
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("https://"));
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .or_else(|| uri.authority().map(|a| a.as_str()));
    if origin.is_none() || origin != host {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "Open the messagebook on this node before posting.",
        ));
    }
    Ok(())
}
async fn save(
    State(state): State<Shared>,
    headers: HeaderMap,
    axum::extract::OriginalUri(uri): axum::extract::OriginalUri,
    Json(input): Json<NewMessage>,
) -> Result<Json<Message>, ApiError> {
    check_origin(&headers, &uri)?;
    let label = input.label.trim().to_owned();
    let text = input.text.trim().to_owned();
    if label.is_empty()
        || label.chars().count() > 48
        || text.is_empty()
        || text.chars().count() > 2000
        || input.request_id.is_empty()
        || input.request_id.len() > 64
        || !input
            .request_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "Use a name up to 48 characters and a message up to 2,000 characters.",
        ));
    }
    with_store(state, move |s| s.append(&input.request_id, &label, &text))
        .await
        .map(Json)
}
fn app(store: Store, web: &Path, ca: &Path) -> Router {
    Router::new()
        .route("/api/node", get(info))
        .route("/api/messages", get(messages).post(save))
        .route_service("/local-ca.pem", tower_http::services::ServeFile::new(ca))
        .fallback_service(ServeDir::new(web).append_index_html_on_directories(true))
        .layer(DefaultBodyLimit::max(16*1024))
        // SvelteKit emits a hash-based script policy in the static HTML. This header
        // adds directives that require an HTTP response (notably frame-ancestors).
        .layer(SetResponseHeaderLayer::overriding(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("connect-src 'self'; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'")))
        .layer(SetResponseHeaderLayer::overriding(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")))
        .layer(SetResponseHeaderLayer::overriding(header::CACHE_CONTROL, HeaderValue::from_static("no-store")))
        .layer(SetResponseHeaderLayer::overriding(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer")))
        .with_state(Arc::new(Mutex::new(store)))
}
fn fixture_key(state: &Path) -> Result<SigningKey, Box<dyn std::error::Error>> {
    fs::create_dir_all(state)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(state, fs::Permissions::from_mode(0o700))?;
    }
    let path = state.join("fixture-signing-key.bin");
    if !path.exists() {
        if state.join("messages.sqlite").exists() {
            return Err("Fixture key is missing. Restore the matching state folder; do not replace it over an existing database.".into());
        }
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&path)?;
        file.write_all(&SigningKey::generate(&mut OsRng).to_bytes())?;
        file.sync_all()?;
    }
    let bytes: [u8; 32] = fs::read(path)?
        .try_into()
        .map_err(|_| "Invalid fixture key length")?;
    Ok(SigningKey::from_bytes(&bytes))
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|_| "TLS provider initialization failed")?;
    let state = PathBuf::from(std::env::var("BBSBOX_STATE").unwrap_or_else(|_| ".state".into()));
    let web = PathBuf::from(std::env::var("BBSBOX_WEB").unwrap_or_else(|_| "web/build".into()));
    let address: SocketAddr = std::env::var("BBSBOX_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8443".into())
        .parse()?;
    if !web.join("index.html").is_file() {
        return Err("Build the frontend first: cd web && npm ci && npm run build".into());
    }
    let tls = axum_server::tls_rustls::RustlsConfig::from_pem_file(
        state.join("tls/server.pem"),
        state.join("tls/server-key.pem"),
    )
    .await
    .map_err(|e| format!("Generate local TLS material with tools/init_tls.py first: {e}"))?;
    let key = fixture_key(&state)?;
    let store = Store::open(&state.join("messages.sqlite"), key)?;
    println!("BBSBOX M00 experiment: https://{address}");
    println!("Node fixture signing key (public): {}", store.public_key());
    println!(
        "Loopback by default. Browser messages use this node fixture key; personal identity and encrypted groups are later work."
    );
    let handle = axum_server::Handle::new();
    let shutdown = handle.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        shutdown.graceful_shutdown(Some(std::time::Duration::from_secs(5)));
    });
    axum_server::bind_rustls(address, tls)
        .handle(handle)
        .serve(app(store, &web, &state.join("tls/ca.pem")).into_make_service())
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    #[test]
    fn origin_check_supports_http2_authority_and_rejects_missing_or_foreign_origins() {
        let uri: axum::http::Uri = "https://localhost:8443/api/messages".parse().unwrap();
        let mut headers = HeaderMap::new();
        assert!(check_origin(&headers, &uri).is_err());
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://localhost:8443"),
        );
        assert!(check_origin(&headers, &uri).is_ok());
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://other.invalid"),
        );
        assert!(check_origin(&headers, &uri).is_err());
    }
    fn fixture() -> (Router, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(
            &dir.path().join("test.sqlite"),
            SigningKey::from_bytes(&[5; 32]),
        )
        .unwrap();
        (app(store, dir.path(), &dir.path().join("ca.pem")), dir)
    }
    #[tokio::test]
    async fn api_rejects_cross_origin_and_empty_posts_and_retains_valid_message() {
        let (app, _dir) = fixture();
        let request = |origin: &str, text: &str| {
            Request::builder()
                .method("POST")
                .uri("/api/messages")
                .header("host", "localhost:8443")
                .header("origin", origin)
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({"request_id":"test-1","label":"Neighbor","text":text})
                        .to_string(),
                ))
                .unwrap()
        };
        assert_eq!(
            app.clone()
                .oneshot(request("https://other.invalid", "Hello"))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            app.clone()
                .oneshot(request("https://localhost:8443", "  "))
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
        let response = app
            .clone()
            .oneshot(request("https://localhost:8443", "Hello"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/messages")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response.headers()[header::CONTENT_SECURITY_POLICY]
                .to_str()
                .unwrap()
                .contains("connect-src 'self'")
        );
        let bytes = axum::body::to_bytes(response.into_body(), 16384)
            .await
            .unwrap();
        let messages: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(messages[0]["verified"], true);
        assert_eq!(messages[0]["text"], "Hello");
    }
}
