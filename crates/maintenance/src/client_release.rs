// SPDX-License-Identifier: AGPL-3.0-or-later
//! Independent client release profile; node release bytes and policy are unchanged.
use crate::{
    Limits,
    inventory::{self, FileRecord},
    release::{Error, KeyId, ReleaseSigner, Result},
};
use coset::{CborSerializable, CoseSign1};
use ed25519_dalek::{Signature, Signer, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const DOMAIN: &str = "bbsbox.client-release/v1";
pub const TARGET: &str = "windows-x86_64-msvc";
pub const ENTRY: &str = "bbsbox-client.exe";
pub const MAX_MANIFEST: usize = 65536;
pub const REQUIRED: [&str; 7] = [
    "LICENSE.md",
    "LICENSES/AGPL-3.0-or-later.txt",
    "LICENSES/CC-BY-SA-4.0.txt",
    ENTRY,
    "materials/dependencies.tar",
    "materials/notices.json",
    "materials/source.tar",
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: String,
    pub issuer: String,
    pub sequence: u64,
    pub version: String,
    pub target: String,
    pub source_commit: String,
    pub vault_profiles: Vec<u32>,
    pub files: Vec<FileRecord>,
    pub next_key: Option<[u8; 32]>,
}
fn digest(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
impl Manifest {
    fn validate(&self) -> Result<()> {
        if self.format != DOMAIN
            || self.target != TARGET
            || self.sequence == 0
            || self.sequence > i64::MAX as u64
            || self.version.is_empty()
            || self.version.len() > 64
            || !self
                .version
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-+".contains(&b))
            || !digest(&self.source_commit, 40)
            || self.vault_profiles != [1]
            || self.files.len() < REQUIRED.len()
            || self.files.len() > 128
        {
            return Err(Error::InvalidManifest);
        }
        let mut previous = None;
        let mut folded = BTreeSet::new();
        let mut aliases = BTreeMap::new();
        let mut total = 0u64;
        for file in &self.files {
            // Windows paths are case-insensitive. Reject aliases even on a Unix verifier.
            if !inventory::valid_path(&file.path)
                || !digest(&file.sha256, 64)
                || previous.is_some_and(|name: &str| name >= file.path.as_str())
                || file.path.eq_ignore_ascii_case("manifest.cose")
                || !folded.insert(file.path.to_ascii_lowercase())
            {
                return Err(Error::InvalidManifest);
            }
            if file.size > 2 * 1024 * 1024 * 1024 {
                return Err(Error::LimitExceeded);
            }
            total = total.checked_add(file.size).ok_or(Error::LimitExceeded)?;
            if total > 4 * 1024 * 1024 * 1024 {
                return Err(Error::LimitExceeded);
            }
            previous = Some(file.path.as_str());
            for end in file
                .path
                .match_indices('/')
                .map(|(at, _)| at)
                .chain([file.path.len()])
            {
                let name = &file.path[..end];
                if aliases
                    .insert(name.to_ascii_lowercase(), name)
                    .is_some_and(|old| old != name)
                {
                    return Err(Error::InvalidManifest);
                }
            }
        }
        // Reject file/directory aliases such as A and a/child on Windows.
        for name in &folded {
            for (at, _) in name.match_indices('/') {
                if folded.contains(&name[..at]) {
                    return Err(Error::InvalidManifest);
                }
            }
        }
        if REQUIRED
            .iter()
            .any(|name| !self.files.iter().any(|f| f.path == *name && f.size > 0))
        {
            return Err(Error::InvalidManifest);
        }
        if let Some(next) = &self.next_key {
            let next = KeyId::from_public(next)?;
            if next.to_string() == self.issuer {
                return Err(Error::InvalidManifest);
            }
        }
        Ok(())
    }
}
pub struct SignedClient {
    encoded: Vec<u8>,
    manifest: Manifest,
    id: [u8; 32],
    signer: [u8; 32],
}
impl SignedClient {
    pub fn encoded(&self) -> &[u8] {
        &self.encoded
    }
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
    pub fn id(&self) -> &[u8; 32] {
        &self.id
    }
    pub fn signer(&self) -> &[u8; 32] {
        &self.signer
    }
}
pub fn sign(manifest: Manifest, signer: &ReleaseSigner) -> Result<SignedClient> {
    if manifest.issuer != signer.id().to_string() {
        return Err(Error::WrongKey);
    }
    manifest.validate()?;
    let payload = serde_json::to_vec(&manifest).map_err(|_| Error::InvalidManifest)?;
    if payload.len() > MAX_MANIFEST {
        return Err(Error::LimitExceeded);
    }
    let mut cose = inventory::unsigned(payload, &signer.public());
    cose.signature = signer
        .key
        .sign(&cose.tbs_data(DOMAIN.as_bytes()))
        .to_bytes()
        .to_vec();
    verify(
        &cose.to_vec().map_err(|_| Error::InvalidManifest)?,
        &signer.public(),
    )
}
pub fn verify(encoded: &[u8], expected: &[u8; 32]) -> Result<SignedClient> {
    KeyId::from_public(expected)?;
    let signer = inventory::preflight(
        encoded,
        Limits {
            max_manifest: MAX_MANIFEST,
            ..Default::default()
        },
    )
    .map_err(|_| Error::InvalidManifest)?;
    if signer != *expected {
        return Err(Error::WrongKey);
    }
    let cose = CoseSign1::from_slice(encoded).map_err(|_| Error::InvalidManifest)?;
    let payload = cose.payload.as_ref().ok_or(Error::InvalidManifest)?;
    let manifest: Manifest = serde_json::from_slice(payload).map_err(|_| Error::InvalidManifest)?;
    manifest.validate()?;
    if manifest.issuer != KeyId::from_public(&signer)?.to_string()
        || serde_json::to_vec(&manifest).map_err(|_| Error::InvalidManifest)? != *payload
    {
        return Err(Error::InvalidManifest);
    }
    let mut canonical = inventory::unsigned(payload.clone(), &signer);
    canonical.signature = cose.signature.clone();
    if canonical.to_vec().map_err(|_| Error::InvalidManifest)? != encoded {
        return Err(Error::InvalidManifest);
    }
    let key = VerifyingKey::from_bytes(&signer).map_err(|_| Error::InvalidKey)?;
    cose.verify_signature(DOMAIN.as_bytes(), |signature, message| {
        key.verify_strict(
            message,
            &Signature::try_from(signature).map_err(|_| Error::InvalidSignature)?,
        )
        .map_err(|_| Error::InvalidSignature)
    })?;
    Ok(SignedClient {
        encoded: encoded.into(),
        manifest,
        id: Sha256::digest(encoded).into(),
        signer,
    })
}
