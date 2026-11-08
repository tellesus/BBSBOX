// SPDX-License-Identifier: AGPL-3.0-or-later
fn main() {
    let mut attributes = tauri_build::Attributes::new();
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // A plain colored 16px icon supplies the resource required by Windows builds.
        let path = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("probe.ico");
        let mut icon = vec![0, 0, 1, 0, 1, 0, 16, 16, 0, 0, 1, 0, 32, 0];
        for value in [1128u32, 22, 40, 16, 32] {
            icon.extend_from_slice(&value.to_le_bytes());
        }
        icon.extend_from_slice(&[1, 0, 32, 0]);
        icon.extend_from_slice(&[0; 24]);
        for _ in 0..256 {
            icon.extend_from_slice(&[211, 65, 255, 255]);
        }
        icon.resize(1150, 0);
        std::fs::write(&path, icon).unwrap();
        attributes = attributes
            .windows_attributes(tauri_build::WindowsAttributes::new().window_icon_path(path));
    }
    tauri_build::try_build(attributes).expect("native check build resources");
}
