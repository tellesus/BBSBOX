// SPDX-License-Identifier: AGPL-3.0-or-later
//! Producer signing and public payload inspection/staging. No installed trust or activation changes.
use crate::Result;
use bbsbox_maintenance::{
    release::{self, Manifest, ReleaseSigner, SignedRelease},
    release_payload,
};
use rand_core::{OsRng, RngCore};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
};
use zeroize::Zeroizing;
const KEY_HEADER: &[u8] = b"BBSBOX-RELEASE-KEY-V1\n";
fn new_file(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if file.metadata()?.permissions().mode() & 0o077 != 0 {
            return Err("release output requires private file modes".into());
        }
    }
    Ok(file)
}
fn sync_parent(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?
    .sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
pub fn keygen(path: &Path) -> Result<serde_json::Value> {
    let mut seed = Zeroizing::new([0; 32]);
    OsRng.fill_bytes(seed.as_mut());
    let signer = ReleaseSigner::from_secret(Zeroizing::new(*seed))?;
    let mut raw = Zeroizing::new(vec![0; KEY_HEADER.len() + 65]);
    raw[..KEY_HEADER.len()].copy_from_slice(KEY_HEADER);
    hex::encode_to_slice(&seed[..], &mut raw[KEY_HEADER.len()..KEY_HEADER.len() + 64])?;
    *raw.last_mut().unwrap() = b'\n';
    let mut output = new_file(path)?;
    output.write_all(&raw)?;
    output.sync_all()?;
    sync_parent(path)?;
    Ok(
        serde_json::json!({"format":"bbsbox.release-public-key/v1","public_key":hex::encode(signer.public()),"key_id":signer.id().to_string()}),
    )
}
fn read_key(path: &Path) -> Result<ReleaseSigner> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() != (KEY_HEADER.len() + 65) as u64
    {
        return Err("release key must be a bounded regular file in its own role format".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err("release key must be private to its owner".into());
        }
    }
    let mut raw = Zeroizing::new(vec![]);
    File::open(path)?.take(1024).read_to_end(&mut raw)?;
    if raw.len() != KEY_HEADER.len() + 65
        || !raw.starts_with(KEY_HEADER)
        || raw.last() != Some(&b'\n')
        || !raw[KEY_HEADER.len()..raw.len() - 1]
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
    {
        return Err("invalid release key role/encoding".into());
    }
    let mut seed = Zeroizing::new([0; 32]);
    hex::decode_to_slice(&raw[KEY_HEADER.len()..raw.len() - 1], seed.as_mut())?;
    Ok(ReleaseSigner::from_secret(seed)?)
}
fn report(signed: &SignedRelease) -> serde_json::Value {
    let manifest = signed.manifest();
    serde_json::json!({"format":"bbsbox.release-inspection/v1","release_id":hex::encode(signed.id()),"issuer":manifest.issuer,"sequence":manifest.sequence,"source_commit":manifest.source_commit,"files":manifest.files.len(),"expected_profile":release_payload::RuntimeProfile{format:release_payload::PROFILE.into(),version:manifest.version.clone(),target:manifest.target.clone(),core_schema:manifest.core_schema,readable_core_schemas:manifest.readable_core_schemas.clone(),module_migrations:manifest.module_migrations.clone()},"installation_authority":false})
}
/// Explicit producer operation. Material semantics and actual compiled-profile checks are external.
pub fn sign(bundle: &Path, manifest_path: &Path, key_path: &Path) -> Result<serde_json::Value> {
    let files = release_payload::inventory_unsigned(bundle)?;
    if key_path.canonicalize()?.starts_with(bundle.canonicalize()?) {
        return Err("keep the release signing key outside its payload".into());
    }
    let metadata = fs::symlink_metadata(manifest_path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("release input must be a regular file".into());
    }
    let mut raw = vec![];
    File::open(manifest_path)?
        .take((release::MAX_MANIFEST + 1) as u64)
        .read_to_end(&mut raw)?;
    if raw.len() > release::MAX_MANIFEST {
        return Err("release input exceeds its bound".into());
    }
    let manifest: Manifest = serde_json::from_slice(&raw)?;
    if manifest.files != files {
        return Err("release input does not inventory the exact prepared payload".into());
    }
    let signer = read_key(key_path)?;
    let signed = release::sign(manifest, &signer)?;
    release_payload::verify_unsigned(bundle, &signed)?;
    let path = bundle.join(release_payload::MANIFEST);
    let mut output = new_file(&path)?;
    output.write_all(signed.encoded())?;
    output.sync_all()?;
    drop(output);
    let verified = release_payload::verify(bundle, &signer.public())?;
    sync_parent(&path)?;
    Ok(report(&verified))
}
pub fn command(arguments: &[OsString]) -> Result<serde_json::Value> {
    let command = arguments
        .first()
        .and_then(|s| s.to_str())
        .ok_or("release command missing")?;
    if command.starts_with("release-control-") {
        return control_command(command, &arguments[1..]);
    }
    let names: &[&str] = match command {
        "release-keygen" => &["--output"],
        "release-sign" => &["--bundle", "--manifest", "--key-file"],
        "release-inspect" => &["--bundle", "--public-key"],
        "release-stage" => &["--bundle", "--public-key", "--output"],
        _ => return Err("unknown release command".into()),
    };
    let mut options = BTreeMap::new();
    let (chunks, remainder) = arguments[1..].as_chunks::<2>();
    if !remainder.is_empty() {
        return Err("each release option requires a value".into());
    }
    for pair in chunks {
        let name = pair[0].to_str().ok_or("invalid release option")?;
        if !names.contains(&name) || options.insert(name, pair[1].as_os_str()).is_some() {
            return Err("unknown or duplicate release option".into());
        }
    }
    if options.len() != names.len() {
        return Err("required release options are missing".into());
    }
    let path = |name| Path::new(options[name]);
    match command {
        "release-keygen" => keygen(path("--output")),
        "release-sign" => sign(path("--bundle"), path("--manifest"), path("--key-file")),
        _ => {
            let raw = options["--public-key"]
                .to_str()
                .ok_or("invalid public key")?;
            if raw.len() != 64
                || !raw
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(
                    "release public key must be 64 lowercase hexadecimal characters".into(),
                );
            }
            let mut public = [0; 32];
            hex::decode_to_slice(raw, &mut public)?;
            let signed = if command == "release-stage" {
                release_payload::stage(path("--bundle"), &public, path("--output"))?
            } else {
                release_payload::verify(path("--bundle"), &public)?
            };
            Ok(report(&signed))
        }
    }
}

fn fixed_hex<const N: usize>(value: &std::ffi::OsStr) -> Result<[u8; N]> {
    let value = value.to_str().ok_or("invalid hexadecimal release option")?;
    if value.len() != N * 2
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("invalid hexadecimal release option".into());
    }
    let mut bytes = [0; N];
    hex::decode_to_slice(value, &mut bytes)?;
    Ok(bytes)
}
fn control_command(command: &str, arguments: &[OsString]) -> Result<serde_json::Value> {
    use bbsbox_maintenance::release_trust::{Recovery, Store};
    if command == "release-control-boot" {
        if !arguments.is_empty() {
            return Err("installed release boot check takes no options".into());
        }
        return serialized_boot();
    }
    if command == "release-control-installed" {
        if !arguments.is_empty() {
            return Err("installed release inspection takes no options".into());
        }
        // Read-only inspection for a coordinator already holding the appliance maintenance lock.
        return installed_boot();
    }
    let names: &[&str] = match command {
        "release-control-init" => &["--directory", "--public-key"],
        "release-control-state" => &["--directory"],
        "release-control-check" => &["--directory", "--release"],
        "release-control-begin" => &["--directory", "--revision", "--bundle", "--request"],
        "release-control-complete" => &["--directory", "--revision", "--request", "--release-id"],
        "release-control-failed" => &["--directory", "--revision", "--request", "--reason", "--at"],
        "release-control-recover" => &[
            "--directory",
            "--revision",
            "--request",
            "--release-id",
            "--snapshot",
            "--reason",
            "--at",
        ],
        _ => return Err("unknown release control command".into()),
    };
    let mut options = BTreeMap::new();
    let (chunks, remainder) = arguments.as_chunks::<2>();
    if !remainder.is_empty() {
        return Err("each release control option requires a value".into());
    }
    for pair in chunks {
        let name = pair[0].to_str().ok_or("invalid release control option")?;
        if !names.contains(&name) || options.insert(name, pair[1].as_os_str()).is_some() {
            return Err("unknown or duplicate release control option".into());
        }
    }
    if options.len() != names.len() {
        return Err("required release control options are missing".into());
    }
    let path = |name| Path::new(options[name]);
    let text = |name| {
        options[name]
            .to_str()
            .ok_or("release control option must be text")
    };
    let store = if command == "release-control-init" {
        Store::initialize(path("--directory"), fixed_hex(options["--public-key"])?)?
    } else {
        Store::open(path("--directory"))?
    };
    if command == "release-control-check" {
        return Ok(
            serde_json::json!({"format":"bbsbox.release-boot-check/v1","selection":store.check_selected(path("--release"))?}),
        );
    }
    let state = match command {
        "release-control-init" | "release-control-state" => store.state()?,
        _ => {
            let revision = text("--revision")?.parse()?;
            let request = fixed_hex(options["--request"])?;
            match command {
                "release-control-begin" => {
                    // Full byte authentication is mandatory even for an explicit local control request.
                    // The coordinator additionally checks material semantics and the actual profile.
                    let current = store.state()?;
                    let signed = release_payload::verify(path("--bundle"), &current.trusted_key)?;
                    store.begin(revision, signed.encoded(), request)?
                }
                "release-control-complete" => {
                    store.complete(revision, request, fixed_hex(options["--release-id"])?)?
                }
                "release-control-failed" => {
                    store.failed(revision, request, text("--reason")?, text("--at")?.parse()?)?
                }
                "release-control-recover" => store.begin_recovery(
                    revision,
                    fixed_hex(options["--release-id"])?,
                    request,
                    Recovery {
                        snapshot_inventory: fixed_hex(options["--snapshot"])?,
                        reason: text("--reason")?.into(),
                        recorded_at: text("--at")?.parse()?,
                    },
                )?,
                _ => return Err("unknown release control transition".into()),
            }
        }
    };
    Ok(serde_json::json!({"format":"bbsbox.release-control/v1","state":state}))
}

/// Read-only installed identity binding. Missing/corrupt storage is never initialized here.
pub fn check_installed_node(
    database: &Path,
    installation: &Path,
) -> Result<bbsbox_credentials::NodeId> {
    for path in [database, installation] {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err("installed node paths must be regular files".into());
        }
    }
    if fs::metadata(installation)?.len() > 65536 {
        return Err("installation metadata exceeds its bound".into());
    }
    let installed: serde_json::Value =
        serde_json::from_reader(File::open(installation)?.take(65537))?;
    if !matches!(
        installed["format"].as_str(),
        Some("bbsbox.install/v1" | "bbsbox.development-install/v1")
    ) {
        return Err("expected node installation metadata".into());
    }
    let expected = installed["node_id"]
        .as_str()
        .ok_or("installed node identity is missing")?
        .parse()?;
    if bbsbox_identity::node::NodeSigner::inspect_existing(database)? != expected {
        return Err("installed node identity differs from its saved registration".into());
    }
    Ok(expected)
}

fn serialized_boot() -> Result<serde_json::Value> {
    #[cfg(not(unix))]
    {
        Err("installed boot serialization requires the Debian appliance".into())
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
        let path = Path::new("/run/bbsbox-maintenance.lock");
        if fs::symlink_metadata(path).is_ok_and(|m| !m.is_file() || m.file_type().is_symlink()) {
            return Err("unexpected maintenance lock path".into());
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(path)?;
        let metadata = file.metadata()?;
        if metadata.uid() != 0 || metadata.mode() & 0o077 != 0 || !metadata.is_file() {
            return Err("maintenance lock must be private and root-owned".into());
        }
        file.try_lock()?;
        let result = installed_boot();
        file.unlock()?;
        result
    }
}

fn installed_boot() -> Result<serde_json::Value> {
    #[cfg(not(unix))]
    {
        Err("installed release boot checks require the Debian appliance".into())
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let protected = |path: &Path, directory: bool| -> Result<()> {
            let metadata = fs::symlink_metadata(path)?;
            if metadata.file_type().is_symlink()
                || metadata.uid() != 0
                || metadata.mode() & 0o022 != 0
                || metadata.is_dir() != directory
                || (!directory && !metadata.is_file())
            {
                return Err("installed release paths must be root-owned and protected".into());
            }
            Ok(())
        };
        let control = Path::new("/var/lib/bbsbox-maintenance/release-control");
        for path in [
            Path::new("/opt/bbsbox"),
            Path::new("/opt/bbsbox/releases"),
            Path::new("/opt/bbsbox/maintenance"),
            Path::new("/var/lib/bbsbox-maintenance"),
            control,
        ] {
            protected(path, true)?;
        }
        protected(Path::new("/opt/bbsbox/maintenance/bbsbox-operator"), false)?;
        protected(&control.join("release-trust.sqlite"), false)?;
        let current = Path::new("/opt/bbsbox/current");
        let link = fs::symlink_metadata(current)?;
        if !link.file_type().is_symlink() || link.uid() != 0 {
            return Err("application selection must be a root-owned symlink".into());
        }
        let selected = current.canonicalize()?;
        if selected.parent() != Some(Path::new("/opt/bbsbox/releases")) {
            return Err("application selection escapes installed releases".into());
        }
        protected(&selected, true)?;
        protected(&selected.join("bbsbox-node"), false)?;
        protected(&selected.join("manifest.cose"), false)?;
        let store = bbsbox_maintenance::release_trust::Store::open(control)?;
        let active = store.check_selected(&selected)?;
        if selected.file_name().and_then(|p| p.to_str())
            != Some(hex::encode(active.release_id).as_str())
        {
            return Err("installed release directory differs from its receipt".into());
        }
        let configuration = Path::new("/etc/bbsbox");
        protected(configuration, true)?;
        protected(&configuration.join("installation.json"), false)?;
        protected(&configuration.join("node.json"), false)?;
        let config = crate::Config::read(&configuration.join("node.json"))?;
        if config.database != Path::new("/var/lib/bbsbox/node.sqlite") {
            return Err("installed node storage differs from the fixed appliance layout".into());
        }
        // Root verifies public registration; the unprivileged CLI checks the actual SQLite key.
        // A root WAL reader here could leave sidecars unreadable by the service account.
        if fs::metadata(configuration.join("installation.json"))?.len() > 65536 {
            return Err("installation metadata exceeds its bound".into());
        }
        let installed: serde_json::Value = serde_json::from_reader(
            File::open(configuration.join("installation.json"))?.take(65537),
        )?;
        if installed["format"] != "bbsbox.install/v1"
            || installed["release_id"] != hex::encode(active.release_id)
        {
            return Err("installation metadata differs from the selected release".into());
        }
        let node: bbsbox_credentials::NodeId = installed["node_id"]
            .as_str()
            .ok_or("installed node identity is missing")?
            .parse()?;
        Ok(
            serde_json::json!({"format":"bbsbox.release-boot-check/v1","selection":active,"expected_node":node.to_string()}),
        )
    }
}
