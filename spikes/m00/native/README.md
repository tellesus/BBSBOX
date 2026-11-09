# M00 native client experiment

To open the built desktop app on Windows, double-click **Start BBSBOX.cmd** in this folder. Opening `ui/index.html` directly in a browser shows launch instructions; identity operations require the desktop app.

This experiment builds a real Tauri 2 client with locally bundled UI and the existing Rust client identity provider. Root/device keys stay in the native process and persist in a passphrase-encrypted local vault. The UI offers create/unlock and reads public identity status through native commands. The automatic check attempts to navigate to replacement web content, then verifies that navigation was denied and the bundled document still owns the window.

On Windows, prepare Microsoft C++ Build Tools, the Windows SDK, WebView2 and the pinned MSVC Rust toolchain. For the repository-local tool installation verified on this workstation, run these commands from the repository root.

```powershell
$env:CARGO_HOME = "$PWD/.tooling/cargo"
$env:RUSTUP_HOME = "$PWD/.tooling/rustup"
New-Item -ItemType Directory -Path work -Force | Out-Null
& .tooling/cargo/bin/cargo.exe +1.99.1-x86_64-pc-windows-msvc build --locked --offline --manifest-path spikes/m00/native/Cargo.toml
python spikes/m00/native/check.py --binary spikes/m00/native/target/debug/bbsbox-native-probe.exe
```

The check uses fresh temporary profiles and several real hidden WebViews. It exercises create, restart, wrong-password refusal, encrypted backup recovery into a new vault and restart of the recovered identity. Running the executable without arguments shows create/unlock for its local application-data profile.

This evidence covers the bundled-document/native-command boundary and denied remote navigation. The [vault format](../../../protocol/client-local-vault-v1.md) records encryption and recovery rules. Independent release signatures, pinned node TLS, client pairing, MLS integration and Android/iOS acceptance remain in M00/M13. The experiment's `Cargo.lock` is independent of the node workspace. [Tauri's prerequisites](https://v2.tauri.app/start/prerequisites/) describe the native build dependencies.
