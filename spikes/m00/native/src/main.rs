// SPDX-License-Identifier: AGPL-3.0-or-later
//! M00 native client trust and encrypted persistence experiment.
mod backend;
use backend::{Backend, Fixture, Status};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use zeroize::Zeroizing;

struct Probe {
    backend: Arc<Backend>,
    output: Option<PathBuf>,
    fixture: Option<Fixture>,
    denied: Arc<AtomicBool>,
}

fn bundled(url: &tauri::Url) -> bool {
    url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && ((url.scheme() == "tauri" && url.host_str() == Some("localhost"))
            || (url.scheme() == "http" && url.host_str() == Some("tauri.localhost")))
}

#[tauri::command]
fn probe_status(state: tauri::State<'_, Probe>) -> Result<Status, String> {
    state.backend.status(state.output.is_some())
}

#[tauri::command]
async fn open_identity(
    state: tauri::State<'_, Probe>,
    passphrase: String,
    create: bool,
) -> Result<Status, String> {
    let pass = Zeroizing::new(passphrase);
    let backend = state.backend.clone();
    let checking = state.output.is_some();
    tauri::async_runtime::spawn_blocking(move || backend.open(pass, create, checking))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
async fn prepare_check(
    app: tauri::AppHandle,
    state: tauri::State<'_, Probe>,
) -> Result<Status, String> {
    let fixture = state
        .fixture
        .clone()
        .ok_or("No automated check requested")?;
    let backend = state.backend.clone();
    let result = tauri::async_runtime::spawn_blocking(move || backend.check(&fixture))
        .await
        .map_err(|error| error.to_string())?;
    if let Err(error) = &result {
        eprintln!("Native client check failed: {error}");
        app.exit(2);
    }
    result
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
    let mut report = state.backend.evidence()?;
    if !bundled(&current)
        || current.as_str() != document_url
        || report["person"].as_str() != Some(&person)
        || !state.denied.load(Ordering::SeqCst)
    {
        return Err("Bundled UI or navigation check failed".into());
    }
    let fields = report.as_object_mut().ok_or("Invalid native report")?;
    fields.extend(
        serde_json::json!({"format":"bbsbox.native-client-probe/v1","bundled_url":current.as_str(),
        "native_identity_command":true,"remote_navigation_denied":true,
        "key_persistence":"encrypted-local-vault/v1","signed_distribution":"pending"})
        .as_object()
        .unwrap()
        .clone(),
    );
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
    let (output, fixture) = match arguments.as_slice() {
        [] => (None, None),
        [option, path, request, input] if option == "--check" && request == "--request" => (
            Some(PathBuf::from(path)),
            Some(serde_json::from_slice::<Fixture>(&backend::read_bounded(
                &PathBuf::from(input),
                16384,
            )?)?),
        ),
        _ => {
            return Err(
                "Usage: bbsbox-native-probe [--check FRESH_REPORT --request FIXTURE_JSON]".into(),
            );
        }
    };
    let checking = output.is_some();
    let denied = Arc::new(AtomicBool::new(false));
    let navigation_denied = denied.clone();
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            probe_status,
            open_identity,
            prepare_check,
            finish_check
        ])
        .setup(move |app| {
            let path = match &fixture {
                Some(fixture) => fixture.vault.clone(),
                None => {
                    let directory = app.path().app_local_data_dir()?;
                    std::fs::create_dir_all(&directory)?;
                    directory.join("identity.sqlite")
                }
            };
            app.manage(Probe {
                backend: Arc::new(Backend::new(path)),
                output,
                fixture,
                denied,
            });
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
