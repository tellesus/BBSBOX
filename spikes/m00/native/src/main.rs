// SPDX-License-Identifier: AGPL-3.0-or-later
//! M00 desktop trust-boundary experiment. Keys are ephemeral and stay in Rust.
use bbsbox_client_identity::RootKey;
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

struct Probe {
    root: RootKey,
    output: Option<PathBuf>,
    denied: Arc<AtomicBool>,
}

#[derive(Serialize)]
struct Status {
    person: String,
    checking: bool,
}

fn bundled(url: &tauri::Url) -> bool {
    url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && ((url.scheme() == "tauri" && url.host_str() == Some("localhost"))
            || (url.scheme() == "http" && url.host_str() == Some("tauri.localhost")))
}

#[tauri::command]
fn probe_status(state: tauri::State<'_, Probe>) -> Status {
    Status {
        person: state.root.person_id().to_string(),
        checking: state.output.is_some(),
    }
}

#[tauri::command]
fn finish_check(
    window: WebviewWindow,
    state: tauri::State<'_, Probe>,
    person: String,
    document_url: String,
) -> Result<(), String> {
    let output = state
        .output
        .as_ref()
        .ok_or("No automated check requested")?;
    let current = window.url().map_err(|error| error.to_string())?;
    if !bundled(&current)
        || current.as_str() != document_url
        || person != state.root.person_id().to_string()
        || !state.denied.load(Ordering::SeqCst)
    {
        return Err("Bundled UI or navigation check failed".into());
    }
    let report = serde_json::json!({
        "format":"bbsbox.native-client-probe/v1", "person":person,
        "bundled_url":current.as_str(), "native_identity_command":true,
        "remote_navigation_denied":true, "key_persistence":"pending",
        "signed_distribution":"pending"
    });
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|error| error.to_string())?;
    serde_json::to_writer_pretty(&mut file, &report).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    window.app_handle().exit(0);
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let output = match arguments.as_slice() {
        [] => None,
        [option, path] if option == "--check" => Some(PathBuf::from(path)),
        _ => return Err("Usage: bbsbox-native-probe [--check FRESH_REPORT_PATH]".into()),
    };
    let checking = output.is_some();
    let denied = Arc::new(AtomicBool::new(false));
    let navigation_denied = denied.clone();
    tauri::Builder::default()
        .manage(Probe {
            root: RootKey::generate(),
            output,
            denied,
        })
        .invoke_handler(tauri::generate_handler![probe_status, finish_check])
        .setup(move |app| {
            WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("BBSBOX Native Check")
                .visible(!checking)
                .on_navigation(move |url| {
                    let allowed = bundled(url);
                    if !allowed && url.as_str() == "https://127.0.0.1:1/replaced-client" {
                        navigation_denied.store(true, Ordering::SeqCst);
                    }
                    allowed
                })
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())?;
    Ok(())
}
