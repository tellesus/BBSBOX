// SPDX-License-Identifier: AGPL-3.0-or-later
//! Offline release-manifest signatures. Payload verification/install/rollback policy are separate layers.
use crate::{
    Limits,
    inventory::{self, FileRecord},
};
use bbsbox_storage::Migration;
use coset::{CborSerializable, CoseSign1};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fmt};
use zeroize::Zeroizing;

pub const DOMAIN: &str = "bbsbox.release-manifest/v1";
pub const MAX_MANIFEST: usize = 65536;
pub const REQUIRED_FILES: [&str; 9] = [
    "LICENSE.md",
    "LICENSES/AGPL-3.0-or-later.txt",
    "LICENSES/CC-BY-SA-4.0.txt",
    "bbsbox-node",
    "bbsbox.service",
    "materials/dependencies.tar",
    "materials/notices.json",
    "materials/source.tar",
    "node.example.json",
];
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug)]
pub enum Error {
    InvalidManifest,
    InvalidSignature,
    InvalidKey,
    WrongKey,
    LimitExceeded,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidManifest => "release manifest is malformed or unsupported",
            Self::InvalidSignature => "release manifest signature is invalid",
            Self::InvalidKey => "release signing key is invalid",
            Self::WrongKey => "release is not signed by the expected release authority",
            Self::LimitExceeded => "release manifest exceeds its bounds",
        })
    }
}
impl std::error::Error for Error {}
fn public(bytes: &[u8; 32]) -> Result<VerifyingKey> {
    let key = VerifyingKey::from_bytes(bytes).map_err(|_| Error::InvalidKey)?;
    if key.is_weak() {
        return Err(Error::InvalidKey);
    }
    Ok(key)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyId([u8; 32]);
impl KeyId {
    pub fn from_public(bytes: &[u8; 32]) -> Result<Self> {
        public(bytes)?;
        let mut hash = Sha256::new();
        hash.update(b"bbsbox.release-signing-key/v1\0");
        hash.update(bytes);
        Ok(Self(hash.finalize().into()))
    }
}
impl fmt::Display for KeyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "release-key:{}", hex::encode(self.0))
    }
}
pub struct ReleaseSigner {
    pub(crate) key: SigningKey,
    id: KeyId,
}
impl ReleaseSigner {
    pub fn generate() -> Result<Self> {
        let mut seed = Zeroizing::new([0; 32]);
        OsRng.fill_bytes(seed.as_mut());
        Self::from_secret(seed)
    }
    /// Load a release seed; operator disk tooling owns its separate key format/protection.
    pub fn from_secret(seed: Zeroizing<[u8; 32]>) -> Result<Self> {
        let key = SigningKey::from_bytes(&seed);
        let id = KeyId::from_public(&key.verifying_key().to_bytes())?;
        Ok(Self { key, id })
    }
    pub fn public(&self) -> [u8; 32] {
        self.key.verifying_key().to_bytes()
    }
    pub fn id(&self) -> KeyId {
        self.id
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleMigration {
    pub module: String,
    pub version: u32,
    pub sha256: String,
}
impl From<&Migration> for ModuleMigration {
    fn from(migration: &Migration) -> Self {
        Self {
            module: migration.module.into(),
            version: migration.version,
            sha256: hex::encode(Sha256::digest(migration.sql.as_bytes())),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: String,
    pub issuer: String,
    pub sequence: u64,
    pub version: String,
    pub target: String,
    pub created_at: i64,
    pub source_commit: String,
    pub core_schema: u32,
    pub readable_core_schemas: Vec<u32>,
    pub module_migrations: Vec<ModuleMigration>,
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
            || self.sequence == 0
            || self.version.is_empty()
            || self.version.len() > 64
            || !self
                .version
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-+".contains(&b))
            || self.target != "debian13-amd64"
            || !digest(&self.source_commit, 40)
            || self.core_schema == 0
            || self.readable_core_schemas.is_empty()
            || self.readable_core_schemas.len() > 16
            || self.readable_core_schemas.contains(&0)
            || self.readable_core_schemas.windows(2).any(|v| v[0] >= v[1])
            || !self.readable_core_schemas.contains(&self.core_schema)
            || self.module_migrations.is_empty()
            || self.module_migrations.len() > 512
        {
            return Err(Error::InvalidManifest);
        }
        let mut previous_module = "";
        let mut previous_version = 0u32;
        for entry in &self.module_migrations {
            if entry.module.is_empty()
                || entry.module.len() > 128
                || !entry
                    .module
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-/.:".contains(&b))
                || !digest(&entry.sha256, 64)
            {
                return Err(Error::InvalidManifest);
            }
            if entry.module.as_str() < previous_module {
                return Err(Error::InvalidManifest);
            }
            if entry.module.as_str() != previous_module {
                previous_module = &entry.module;
                previous_version = 0;
            }
            if previous_version.checked_add(1) != Some(entry.version) {
                return Err(Error::InvalidManifest);
            }
            previous_version = entry.version;
        }
        if self.files.len() < REQUIRED_FILES.len() || self.files.len() > 128 {
            return Err(Error::InvalidManifest);
        }
        let mut previous: Option<&str> = None;
        let mut total = 0u64;
        let mut files = BTreeMap::new();
        for file in &self.files {
            if !inventory::valid_path(&file.path)
                || !digest(&file.sha256, 64)
                || previous.is_some_and(|p| p >= file.path.as_str())
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
            previous = Some(&file.path);
            files.insert(file.path.as_str(), file);
        }
        if REQUIRED_FILES
            .iter()
            .any(|path| files.get(path).is_none_or(|file| file.size == 0))
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
pub struct SignedRelease {
    encoded: Vec<u8>,
    manifest: Manifest,
    id: [u8; 32],
    public: [u8; 32],
}
impl SignedRelease {
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
        &self.public
    }
}
pub fn sign(manifest: Manifest, signer: &ReleaseSigner) -> Result<SignedRelease> {
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
    verify_manifest(
        &cose.to_vec().map_err(|_| Error::InvalidManifest)?,
        &signer.public(),
    )
}
/// Verifies manifest bytes/key/domain only. Installation also checks every payload, compatibility,
/// persisted anti-rollback state and any recorded recovery override before mutating the appliance.
pub fn verify_manifest(encoded: &[u8], expected: &[u8; 32]) -> Result<SignedRelease> {
    public(expected)?;
    let signer = inventory::preflight(
        encoded,
        Limits {
            max_manifest: MAX_MANIFEST,
            ..Limits::default()
        },
    )
    .map_err(|_| Error::InvalidManifest)?;
    if signer != *expected {
        return Err(Error::WrongKey);
    }
    let cose = CoseSign1::from_slice(encoded).map_err(|_| Error::InvalidManifest)?;
    let payload = cose.payload.as_ref().ok_or(Error::InvalidManifest)?;
    let manifest: Manifest = serde_json::from_slice(payload).map_err(|_| Error::InvalidManifest)?;
    if manifest.issuer != KeyId::from_public(&signer)?.to_string() {
        return Err(Error::WrongKey);
    }
    manifest.validate()?;
    if serde_json::to_vec(&manifest).map_err(|_| Error::InvalidManifest)? != *payload {
        return Err(Error::InvalidManifest);
    }
    let mut canonical = inventory::unsigned(payload.clone(), &signer);
    canonical.signature = cose.signature.clone();
    if canonical.to_vec().map_err(|_| Error::InvalidManifest)? != encoded {
        return Err(Error::InvalidManifest);
    }
    let key = public(&signer)?;
    cose.verify_signature(DOMAIN.as_bytes(), |signature, message| {
        key.verify_strict(
            message,
            &Signature::try_from(signature).map_err(|_| Error::InvalidSignature)?,
        )
        .map_err(|_| Error::InvalidSignature)
    })?;
    Ok(SignedRelease {
        encoded: encoded.into(),
        manifest,
        id: Sha256::digest(encoded).into(),
        public: signer,
    })
}
