// SPDX-License-Identifier: AGPL-3.0-or-later
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use bbsbox_node_runtime::{Config, Runtime, app};
use tower::ServiceExt;
fn config(directory: &std::path::Path) -> Config {
    Config {
        format: "bbsbox.node-config/v1".into(),
        name: "Neighborhood <node>".into(),
        bind: "127.0.0.1:8443".parse().unwrap(),
        additional_binds: vec![],
        origin: "https://localhost:8443".into(),
        additional_origins: vec![],
        database: directory.join("node.sqlite"),
        certificate: directory.join("server.pem"),
        certificate_key: directory.join("server-key.pem"),
    }
}
#[test]
fn config_requires_bounded_known_fields_local_literal_addresses_https_and_distinct_absolute_paths()
{
    let temp = tempfile::tempdir().unwrap();
    let valid = config(temp.path());
    valid.validate().unwrap();
    for (bind, origin) in [
        ("0.0.0.0:8443", "https://localhost:8443"),
        ("8.8.8.8:8443", "https://localhost:8443"),
        ("[2001:4860::1]:8443", "https://localhost:8443"),
        ("127.0.0.1:443", "https://localhost"),
        ("127.0.0.1:8443", "http://localhost:8443"),
        ("127.0.0.1:8443", "https://localhost:8443/path"),
        ("127.0.0.1:8443", "https://localhost:8443/"),
        ("127.0.0.1:8443", "https://localhost:9443"),
    ] {
        let mut c = valid.clone();
        c.bind = bind.parse().unwrap();
        c.origin = origin.into();
        assert!(c.validate().is_err(), "{bind} {origin}");
    }
    for bind in ["192.168.4.1:8443", "[fd00::1]:8443", "[::1]:8443"] {
        let mut c = valid.clone();
        c.bind = bind.parse().unwrap();
        assert!(c.validate().is_ok());
    }
    for bind in [
        "0.0.0.0:8443",
        "8.8.8.8:8443",
        "[::1]:9443",
        "127.0.0.1:8443",
    ] {
        let mut c = valid.clone();
        c.additional_binds.push(bind.parse().unwrap());
        assert!(c.validate().is_err());
    }
    let mut c = valid.clone();
    c.database = "relative.sqlite".into();
    assert!(c.validate().is_err());
    c = valid.clone();
    c.certificate = c.certificate_key.clone();
    assert!(c.validate().is_err());
    c = valid.clone();
    c.name = "control\ntext".into();
    assert!(c.validate().is_err());
    let path = temp.path().join("oversized.json");
    std::fs::write(&path, vec![b' '; 65537]).unwrap();
    assert!(Config::read(&path).is_err());
    std::fs::write(
        &path,
        b"{\"format\":\"bbsbox.node-config/v1\",\"unexpected\":true}",
    )
    .unwrap();
    assert!(Config::read(&path).is_err());
    std::fs::write(&path, serde_json::to_vec(&valid).unwrap()).unwrap();
    assert!(Config::read(&path).unwrap().additional_origins.is_empty());
    for origins in [
        vec!["https://localhost:8443"],
        vec!["https://127.0.0.1:9443"],
        vec!["https://127.0.0.1:8443/path"],
        vec!["http://127.0.0.1:8443"],
        vec!["https://127.0.0.1:8443/"],
        vec!["https://127.0.0.1:8443"; 9],
    ] {
        let mut c = valid.clone();
        c.additional_origins = origins.into_iter().map(String::from).collect();
        assert!(c.validate().is_err());
    }
}
#[tokio::test]
async fn lifecycle_uses_real_registered_storage_and_preserves_identity_with_one_writer() {
    let temp = tempfile::tempdir().unwrap();
    let c = config(temp.path());
    let first = Runtime::start(&c).await.unwrap();
    let id = first.info().node_id.clone();
    assert!(Runtime::start(&c).await.is_err());
    first.shutdown().await.unwrap();
    let again = Runtime::start(&c).await.unwrap();
    assert_eq!(again.info().node_id, id);
    again.shutdown().await.unwrap();
    let reader = bbsbox_storage::open_reader(&c.database).unwrap();
    assert_eq!(
        reader
            .query_row(
                "SELECT count(*) FROM core_migrations WHERE module='events'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        3
    );
}

#[tokio::test]
async fn compiled_registry_drives_both_runtime_migrations_and_its_release_profile() {
    let directory = tempfile::tempdir().unwrap();
    let config = config(directory.path());
    let original = Runtime::start(&config).await.unwrap();
    let node = original.info().node_id.clone();
    original.shutdown().await.unwrap();
    let mut registry = bbsbox_node_runtime::migrations();
    registry.push(bbsbox_storage::Migration{module:"events",version:4,sql:"CREATE TABLE events_assembly_fixture(value INTEGER NOT NULL); INSERT INTO events_assembly_fixture VALUES(23);"});
    let profile = bbsbox_node_runtime::release_profile_for_registry(&registry);
    let upgraded = Runtime::start_with_registry(&config, registry)
        .await
        .unwrap();
    assert_eq!(upgraded.info().node_id, node);
    upgraded.shutdown().await.unwrap();
    let reader = bbsbox_storage::open_reader(&config.database).unwrap();
    assert_eq!(
        reader
            .query_row("SELECT value FROM events_assembly_fixture", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        23
    );
    let actual: Vec<u8> = reader
        .query_row(
            "SELECT digest FROM core_migrations WHERE module='events' AND version=4",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        hex::encode(actual),
        profile
            .module_migrations
            .iter()
            .find(|m| m.module == "events" && m.version == 4)
            .unwrap()
            .sha256
    );
    drop(reader);
    assert!(
        Runtime::start(&config).await.is_err(),
        "old assembly must refuse the newer applied registry"
    );
}
#[tokio::test]
async fn status_surface_uses_exact_authority_and_exposes_no_private_state_or_mutation_route() {
    let temp = tempfile::tempdir().unwrap();
    let mut c = config(temp.path());
    c.additional_origins = vec![
        "https://127.0.0.1:8443".into(),
        "https://bbsbox-test.home.arpa:8443".into(),
    ];
    let runtime = Runtime::start(&c).await.unwrap();
    let router = app(&c, runtime.info().clone()).unwrap();
    let request = |path: &str, host: &str, method: &str| {
        Request::builder()
            .uri(path)
            .header("host", host)
            .method(method)
            .body(Body::empty())
            .unwrap()
    };
    let response = router
        .clone()
        .oneshot(request("/api/v1/node", "localhost:8443", "GET"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert!(
        response.headers()[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap()
            .contains("default-src 'none'")
    );
    let bytes = to_bytes(response.into_body(), 8192).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let keys = json
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        keys,
        ["format", "name", "node_id", "status", "version"]
            .into_iter()
            .collect()
    );
    assert_eq!(json["name"], c.name);
    let contract: serde_json::Value = serde_json::from_str(include_str!(
        "../../../protocol/http/node-runtime.openapi.json"
    ))
    .unwrap();
    let declared = contract["components"]["schemas"]["NodeInfo"]["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(keys, declared);
    for host in ["127.0.0.1:8443", "bbsbox-test.home.arpa:8443"] {
        assert_eq!(
            router
                .clone()
                .oneshot(request("/api/v1/node", host, "GET"))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
    }
    for (uri, host) in [
        ("https://localhost:8443/api/v1/node", "127.0.0.1:8443"),
        ("http://localhost:8443/api/v1/node", "localhost:8443"),
    ] {
        assert_eq!(
            router
                .clone()
                .oneshot(request(uri, host, "GET"))
                .await
                .unwrap()
                .status(),
            StatusCode::MISDIRECTED_REQUEST
        );
    }
    let duplicate = Request::builder()
        .uri("/api/v1/node")
        .header("host", "localhost:8443")
        .header("host", "127.0.0.1:8443")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        router.clone().oneshot(duplicate).await.unwrap().status(),
        StatusCode::MISDIRECTED_REQUEST
    );
    assert_eq!(
        router
            .clone()
            .oneshot(request("/api/v1/node", "foreign.invalid", "GET"))
            .await
            .unwrap()
            .status(),
        StatusCode::MISDIRECTED_REQUEST
    );
    let conflicting = Request::builder()
        .uri("https://localhost:8443/api/v1/node")
        .header("host", "foreign.invalid")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        router.clone().oneshot(conflicting).await.unwrap().status(),
        StatusCode::MISDIRECTED_REQUEST
    );
    assert_eq!(
        router
            .clone()
            .oneshot(request("/api/v1/node", "localhost:8443", "POST"))
            .await
            .unwrap()
            .status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
    for (path, mime) in [
        ("/", "text/html"),
        ("/assets/status.js", "text/javascript"),
        ("/assets/status.css", "text/css"),
    ] {
        let response = router
            .clone()
            .oneshot(request(path, "localhost:8443", "GET"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response.headers()[header::CONTENT_TYPE]
                .to_str()
                .unwrap()
                .starts_with(mime)
        );
    }
    runtime.shutdown().await.unwrap();
}

#[tokio::test]
async fn offline_initialization_prepares_identity_without_tls_or_a_listener_and_retains_it_at_boot()
{
    let temp = tempfile::tempdir().unwrap();
    let c = config(temp.path());
    let config_path = temp.path().join("config.json");
    std::fs::write(&config_path, serde_json::to_vec(&c).unwrap()).unwrap();
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_bbsbox-node"));
    command.args(["--config", config_path.to_str().unwrap(), "--initialize"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let initialized: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(initialized["format"], "bbsbox.node-init/v1");
    assert!(!c.certificate.exists());
    assert!(!c.certificate_key.exists());
    let runtime = Runtime::start(&c).await.unwrap();
    assert_eq!(initialized["node_id"], runtime.info().node_id);
    runtime.shutdown().await.unwrap();
}
