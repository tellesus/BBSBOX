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
pub mod cli;
pub mod operator;
pub mod release_operator;
/// No filesystem, identity or listener is opened when reporting compiled compatibility.
pub fn release_profile() -> bbsbox_maintenance::release_payload::RuntimeProfile {
    release_profile_for_registry(&migrations())
}
/// Trusted first-party assembly uses one compiled registry for reporting and execution.
pub fn release_profile_for_registry(
    registry: &[Migration],
) -> bbsbox_maintenance::release_payload::RuntimeProfile {
    let mut module_migrations = registry
        .iter()
        .map(bbsbox_maintenance::release::ModuleMigration::from)
        .collect::<Vec<_>>();
    module_migrations.sort_by(|a, b| (&a.module, a.version).cmp(&(&b.module, b.version)));
    bbsbox_maintenance::release_payload::RuntimeProfile {
        format: bbsbox_maintenance::release_payload::PROFILE.into(),
        version: env!("CARGO_PKG_VERSION").into(),
        target: if cfg!(all(
            target_os = "linux",
            target_arch = "x86_64",
            target_env = "gnu"
        )) {
            "debian13-amd64".into()
        } else {
            format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
        },
        core_schema: bbsbox_storage::schema_version(),
        // Conservative direct-reader claim. Earlier schemas require migration, not rollback reads.
        readable_core_schemas: vec![bbsbox_storage::schema_version()],
        module_migrations,
    }
}
const MAX_CONFIG: u64 = 65536;
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub format: String,
    pub name: String,
    pub bind: SocketAddr,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_binds: Vec<SocketAddr>,
    pub origin: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_origins: Vec<String>,
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
        if self.additional_binds.len() > 8 {
            return Err("at most eight additional listeners are supported".into());
        }
        let mut listeners = std::collections::BTreeSet::new();
        for bind in std::iter::once(&self.bind).chain(&self.additional_binds) {
            let local = match bind.ip() {
                IpAddr::V4(ip) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
                IpAddr::V6(ip) => {
                    ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local()
                }
            };
            if !local
                || bind.port() < 1024
                || bind.port() != self.bind.port()
                || !listeners.insert(bind)
            {
                return Err("bind must be an explicit local address and unprivileged port".into());
            }
        }
        if self.additional_origins.len() > 8 {
            return Err("at most eight additional local origins are supported".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for origin in std::iter::once(&self.origin).chain(&self.additional_origins) {
            let authority = self.origin_authority(origin)?;
            if !seen.insert(authority) {
                return Err("duplicate local origin".into());
            }
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
    fn origin_authority(&self, origin: &str) -> Result<String> {
        let uri: Uri = origin.parse()?;
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
        if origin.ends_with('/') {
            return Err("origin must omit its trailing slash".into());
        }
        Ok(authority.as_str().into())
    }
    fn authorities(&self) -> Result<Vec<String>> {
        self.validate()?;
        std::iter::once(&self.origin)
            .chain(&self.additional_origins)
            .map(|origin| self.origin_authority(origin))
            .collect()
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
    authorities: Arc<[String]>,
}
pub struct Runtime {
    writer: Writer,
    _signer: NodeSigner,
    info: NodeInfo,
}
impl Runtime {
    pub async fn start(config: &Config) -> Result<Self> {
        Self::start_with_registry(config, migrations()).await
    }
    pub async fn start_with_registry(config: &Config, registry: Vec<Migration>) -> Result<Self> {
        config.validate()?;
        let writer = Writer::start(&config.database, registry, WriterConfig::default())?;
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
    let authority = request.uri().authority().map(|a| a.as_str()).or_else(|| {
        request
            .headers()
            .get(header::HOST)
            .and_then(|v| v.to_str().ok())
    });
    let conflicting_host = request
        .headers()
        .get_all(header::HOST)
        .iter()
        .any(|value| value.to_str().ok() != authority);
    let known = authority.is_some_and(|value| state.authorities.iter().any(|a| a == value));
    let invalid_scheme = request.uri().scheme_str().is_some_and(|s| s != "https");
    let mut response = if conflicting_host || !known || invalid_scheme {
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
        authorities: config.authorities()?.into(),
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
    serve_with_registry(config, migrations()).await
}
pub async fn serve_with_registry(config: Config, registry: Vec<Migration>) -> Result<()> {
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
    // Bind every configured address before activating any HTTP service. A partial
    // listener set must never advertise readiness after another bind fails.
    let mut prepared = Vec::new();
    let mut handles = Vec::new();
    for address in std::iter::once(&config.bind).chain(&config.additional_binds) {
        let listener = tokio::net::TcpListener::bind(address).await?.into_std()?;
        let handle = axum_server::Handle::new();
        prepared.push(axum_server::from_tcp_rustls(listener, tls.clone())?.handle(handle.clone()));
        handles.push(handle);
    }
    let runtime = Runtime::start_with_registry(&config, registry).await?;
    let router = app(&config, runtime.info().clone())?;
    let mut servers = tokio::task::JoinSet::new();
    for server in prepared {
        let service = router.clone().into_make_service();
        servers.spawn(async move { server.serve(service).await });
    }
    println!("BBSBOX {} at {}", runtime.info().node_id, config.origin);
    let mut outcome: Result<()> = tokio::select! {
        result = stop_signal() => result,
        result = servers.join_next() => match result.expect("at least one server") {
            Ok(result) => result.map_err(Into::into),
            Err(error) => Err(error.into()),
        },
    };
    for handle in handles {
        handle.graceful_shutdown(Some(Duration::from_secs(10)));
    }
    while let Some(result) = servers.join_next().await {
        let completion: Result<()> = match result {
            Ok(result) => result.map_err(Into::into),
            Err(error) => Err(error.into()),
        };
        if outcome.is_ok() {
            outcome = completion;
        }
    }
    runtime.shutdown().await?;
    outcome
}
