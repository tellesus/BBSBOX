# M00 native client experiment

This experiment builds a real Tauri 2 client with locally bundled UI and the existing Rust client identity provider. A generated person root stays in the native process. The UI reads its public PersonId through a native command. The automatic check attempts to navigate to replacement web content, then verifies that navigation was denied and the bundled document still owns the window.

On Windows, prepare Microsoft C++ Build Tools, the Windows SDK, WebView2 and the pinned MSVC Rust toolchain. From the repository root, using its configured Cargo/Rustup directories, run these commands.

```powershell
cargo +1.99.1-x86_64-pc-windows-msvc build --locked --offline --manifest-path spikes/m00/native/Cargo.toml
python spikes/m00/native/check.py --binary spikes/m00/native/target/debug/bbsbox-native-probe.exe
```

The check opens a hidden WebView and writes a fresh temporary JSON report before exiting. Running the executable without arguments shows the experiment window.

This evidence covers the bundled-document/native-command boundary and denied remote navigation. Persistent client keys, independent release signatures, pinned node TLS, client pairing and Android/iOS acceptance remain in M00/M13. The experiment's `Cargo.lock` is independent of the node workspace. [Tauri's prerequisites](https://v2.tauri.app/start/prerequisites/) describe the native build dependencies.
