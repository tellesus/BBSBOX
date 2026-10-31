// SPDX-License-Identifier: AGPL-3.0-or-later
//! M01 node lifecycle and HTTPS status surface. Community routes are owned by later modules.
use axum::{
    Json, Router,
    body::Body,
    extract::{Request, State},
    http::{HeaderValue, StatusCode, Uri, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use bbsbox_identity::node::NodeSigner;
use bbsbox_storage::{
    Migration,
    writer::{Config as WriterConfig, Priority, Writer},
};
use serde::{Deserialize, Serialize};
use std::{
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
const MAX_CONFIG: u64 = 65536;
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub format: String,
    pub name: String,
    pub bind: SocketAddr,
    pub origin: String,
    pub database: PathBuf,
    pub certificate: PathBuf,
    pub certificate_key: PathBuf,
}
impl Config {
    pub fn read(path: &Path) -> Result<Self> {
        use std::io::Read;
        let file = std::fs::File::open(path)?;
        let mut bytes = vec![];
        file.take(MAX_CONFIG + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_CONFIG {
            return Err("node configuration exceeds 64 KiB".into());
        }
        let config: Self = serde_json::from_slice(&bytes)?;
        config.validate()?;
        Ok(config)
    }
    pub fn validate(&self) -> Result<()> {
        if self.format != "bbsbox.node-config/v1"
            || self.name.trim().is_empty()
            || self.name.chars().count() > 64
            || self.name.chars().any(char::is_control)
        {
            return Err("invalid node configuration format/name".into());
        }
        let local = match self.bind.ip() {
            IpAddr::V4(ip) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
            IpAddr::V6(ip) => {
                ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local()
            }
        };
        if !local || self.bind.port() < 1024 {
            return Err("bind must be an explicit local address and unprivileged port".into());
        }
        let uri: Uri = self.origin.parse()?;
        let authority = uri.authority().ok_or("HTTPS origin has no authority")?;
        let port = authority.port_u16().unwrap_or(443);
        if uri.scheme_str() != Some("https")
            || uri.path_and_query().is_some_and(|p| p.as_str() != "/")
            || authority.as_str().contains('@')
            || authority.host().is_empty()
            || port != self.bind.port()
        {
            return Err("origin must be a bare HTTPS origin matching the listening port".into());
        }
        if self.origin.ends_with('/') {
            return Err("origin must omit its trailing slash".into());
        }
        for path in [&self.database, &self.certificate, &self.certificate_key] {
            if !path.is_absolute()
                || path
                    .components()
                    .any(|p| matches!(p, std::path::Component::ParentDir))
            {
                return Err("node paths must be absolute without parent traversal".into());
            }
        }
        if self.certificate == self.certificate_key
            || self.database == self.certificate
            || self.database == self.certificate_key
        {
            return Err("node storage and TLS paths must be distinct".into());
        }
        Ok(())
    }
    fn authority(&self) -> Result<String> {
        self.validate()?;
        Ok(self
            .origin
            .parse::<Uri>()?
            .authority()
            .ok_or("missing local authority")?
            .as_str()
            .into())
    }
}
pub fn migrations() -> Vec<Migration> {
    let mut migrations = bbsbox_identity::migrations();
    migrations.extend(bbsbox_event_core::migrations());
    migrations
}
#[derive(Clone, Serialize)]
pub struct NodeInfo {
    pub format: &'static str,
    pub name: String,
    pub node_id: String,
    pub version: &'static str,
    pub status: &'static str,
}
#[derive(Clone)]
struct AppState {
    info: Arc<NodeInfo>,
    authority: Arc<str>,
}
pub struct Runtime {
    writer: Writer,
    _signer: NodeSigner,
    info: NodeInfo,
}
impl Runtime {
    pub async fn start(config: &Config) -> Result<Self> {
        config.validate()?;
        let writer = Writer::start(&config.database, migrations(), WriterConfig::default())?;
        let signer = writer
            .handle()
            .submit_service(Priority::Interactive, NodeSigner::start)?
            .wait()
            .await?;
        let info = NodeInfo {
            format: "bbsbox.node-info/v1",
            name: config.name.clone(),
            node_id: signer.id().to_string(),
            version: env!("CARGO_PKG_VERSION"),
            status: "ready",
        };
        Ok(Self {
            writer,
            _signer: signer,
            info,
        })
    }
    pub fn info(&self) -> &NodeInfo {
        &self.info
    }
    pub async fn shutdown(self) -> Result<()> {
        tokio::task::spawn_blocking(move || self.writer.shutdown()).await??;
        Ok(())
    }
}
async fn health(State(state): State<AppState>) -> Json<NodeInfo> {
    Json(state.info.as_ref().clone())
}
async fn policy(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let conflicting_host = request
        .headers()
        .get_all(header::HOST)
        .iter()
        .any(|value| value.to_str().ok() != Some(state.authority.as_ref()));
    let authority = request.uri().authority().map(|a| a.as_str()).or_else(|| {
        request
            .headers()
            .get(header::HOST)
            .and_then(|v| v.to_str().ok())
    });
    let mut response = if conflicting_host || authority != Some(state.authority.as_ref()) {
        (
            StatusCode::MISDIRECTED_REQUEST,
            "Unknown local node address.",
        )
            .into_response()
    } else {
        next.run(request).await
    };
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(header::CONTENT_SECURITY_POLICY,HeaderValue::from_static("default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'"));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response
}
fn asset(content: &'static str, mime: &'static str) -> Response {
    Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .body(Body::from(content))
        .expect("fixed asset headers")
}
pub fn app(config: &Config, info: NodeInfo) -> Result<Router> {
    let state = AppState {
        info: Arc::new(info),
        authority: config.authority()?.into(),
    };
    Ok(Router::new()
        .route("/", get(|| async { Html(include_str!("status.html")) }))
        .route("/api/v1/node", get(health))
        .route(
            "/assets/status.css",
            get(|| async { asset(include_str!("status.css"), "text/css; charset=utf-8") }),
        )
        .route(
            "/assets/status.js",
            get(|| async { asset(include_str!("status.js"), "text/javascript; charset=utf-8") }),
        )
        .with_state(state.clone())
        .layer(middleware::from_fn_with_state(state, policy)))
}
async fn stop_signal() -> Result<()> {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {result=tokio::signal::ctrl_c()=>{result?;},_=term.recv()=>{}}
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await?;
    }
    Ok(())
}
pub async fn serve(config: Config) -> Result<()> {
    config.validate()?;
    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|_| "TLS provider initialization failed")?;
    // Reject unusable TLS material before creating node storage or opening a listener.
    let tls = axum_server::tls_rustls::RustlsConfig::from_pem_file(
        &config.certificate,
        &config.certificate_key,
    )
    .await?;
    let runtime = Runtime::start(&config).await?;
    let handle = axum_server::Handle::new();
    let shutdown = handle.clone();
    let signal = tokio::spawn(async move {
        if let Err(error) = stop_signal().await {
            eprintln!("stop signal setup failed: {error}");
        }
        shutdown.graceful_shutdown(Some(Duration::from_secs(10)));
    });
    println!("BBSBOX {} at {}", runtime.info().node_id, config.origin);
    let result = axum_server::bind_rustls(config.bind, tls)
        .handle(handle)
        .serve(app(&config, runtime.info().clone())?.into_make_service())
        .await;
    signal.abort();
    runtime.shutdown().await?;
    result?;
    Ok(())
}
