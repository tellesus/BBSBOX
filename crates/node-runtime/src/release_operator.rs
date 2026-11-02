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
