// SPDX-License-Identifier: AGPL-3.0-or-later
//! Canonical signed inventory of staged files; this alone is not a complete-node snapshot claim.
use crate::{Error, Limits, Result};
use bbsbox_credentials::NodeId;
use bbsbox_identity::node::NodeSigner;
use coset::{CborSerializable, CoseSign1, CoseSign1Builder, HeaderBuilder, iana};
use ed25519_dalek::{Signature, VerifyingKey};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const DOMAIN: &str = "bbsbox.archive-inventory/v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileRecord {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inventory {
    pub format: String,
    pub node: String,
    pub archive: String,
    pub created_at: i64,
    pub incarnation: [u8; 16],
    pub files: Vec<FileRecord>,
}
pub struct SignedInventory {
    encoded: Vec<u8>,
    inventory: Inventory,
    id: [u8; 32],
    node: NodeId,
}
impl SignedInventory {
    pub fn encoded(&self) -> &[u8] {
        &self.encoded
    }
    pub fn inventory(&self) -> &Inventory {
        &self.inventory
    }
    pub fn id(&self) -> &[u8; 32] {
        &self.id
    }
    pub fn node(&self) -> NodeId {
        self.node
    }
}

pub(crate) fn valid_path(path: &str) -> bool {
    // Fixed short ASCII names keep this profile portable and avoid archive path extensions.
    !path.is_empty()
        && path.len() <= 94
        && path.split('/').all(|part| {
            if part.is_empty()
                || matches!(part, "." | "..")
                || part.ends_with('.')
                || !part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            {
                return false;
            }
            let name = part.split('.').next().unwrap_or("").to_ascii_uppercase();
            !matches!(name.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                && !(name.len() == 4
                    && (name.starts_with("COM") || name.starts_with("LPT"))
                    && matches!(name.as_bytes()[3], b'1'..=b'9'))
        })
}
impl Inventory {
    pub fn total_bytes(&self) -> Result<u64> {
        self.files.iter().try_fold(0u64, |sum, file| {
            sum.checked_add(file.size).ok_or(Error::LimitExceeded)
        })
    }
    fn validate(&self, limits: Limits) -> Result<()> {
        limits.validate()?;
        if self.format != DOMAIN
            || self.archive.len() != 32
            || !self
                .archive
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            || self.archive.bytes().all(|b| b == b'0')
            || self.incarnation == [0; 16]
            || self.files.is_empty()
        {
            return Err(Error::InvalidArchive);
        }
        if self.files.len() > limits.max_files || self.total_bytes()? > limits.max_total_bytes {
            return Err(Error::LimitExceeded);
        }
        let mut previous: Option<&str> = None;
        for file in &self.files {
            if !valid_path(&file.path) {
                return Err(Error::UnsafePath);
            }
            if previous.is_some_and(|old| old >= file.path.as_str())
                || file.sha256.len() != 64
                || !file
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            {
                return Err(Error::InvalidArchive);
            }
            if file.size > limits.max_member_bytes {
                return Err(Error::LimitExceeded);
            }
            previous = Some(&file.path);
        }
        Ok(())
    }
}
pub(crate) fn unsigned(payload: Vec<u8>, public: &[u8; 32]) -> CoseSign1 {
    CoseSign1Builder::new()
        .protected(
            HeaderBuilder::new()
                .algorithm(iana::Algorithm::Ed25519)
                .key_id(public.to_vec())
                .build(),
        )
        .payload(payload)
        .build()
}
pub fn sign(
    files: Vec<FileRecord>,
    created_at: i64,
    signer: &NodeSigner,
    limits: Limits,
) -> Result<SignedInventory> {
    let mut id = [0; 16];
    while id == [0; 16] {
        OsRng.fill_bytes(&mut id);
    }
    let inventory = Inventory {
        format: DOMAIN.into(),
        node: signer.id().to_string(),
        archive: hex::encode(id),
        created_at,
        incarnation: *signer.incarnation(),
        files,
    };
    inventory.validate(limits)?;
    let payload = serde_json::to_vec(&inventory).map_err(|_| Error::InvalidArchive)?;
    if payload.len() > limits.max_manifest {
        return Err(Error::LimitExceeded);
    }
    let mut cose = unsigned(payload, &signer.public());
    cose.signature = signer
        .sign_current(signer.incarnation(), &cose.tbs_data(DOMAIN.as_bytes()))?
        .to_vec();
    let encoded = cose.to_vec().map_err(|_| Error::InvalidArchive)?;
    verify(&encoded, signer.id(), limits)
}
fn bytes<'a>(input: &'a [u8], at: &mut usize, limit: usize) -> Result<&'a [u8]> {
    let head = *input.get(*at).ok_or(Error::InvalidArchive)?;
    *at += 1;
    if head >> 5 != 2 {
        return Err(Error::InvalidArchive);
    }
    let length = match head & 31 {
        value @ 0..=23 => value as usize,
        24..=26 => {
            let width = 1usize << ((head & 31) - 24); // 1, 2 or 4 length bytes.
            let end = at.checked_add(width).ok_or(Error::InvalidArchive)?;
            let raw = input.get(*at..end).ok_or(Error::InvalidArchive)?;
            *at = end;
            let value = raw.iter().fold(0usize, |n, b| (n << 8) | *b as usize);
            let minimum = match width {
                1 => 24,
                2 => 256,
                4 => 65536,
                _ => unreachable!(),
            };
            if value < minimum {
                return Err(Error::InvalidArchive);
            }
            value
        }
        _ => return Err(Error::InvalidArchive),
    };
    if length > limit {
        return Err(Error::LimitExceeded);
    }
    let end = at.checked_add(length).ok_or(Error::InvalidArchive)?;
    let result = input.get(*at..end).ok_or(Error::InvalidArchive)?;
    *at = end;
    Ok(result)
}
pub(crate) fn preflight(encoded: &[u8], limits: Limits) -> Result<[u8; 32]> {
    limits.validate()?;
    if encoded.len() > limits.max_manifest + 128 || encoded.first() != Some(&0x84) {
        return Err(Error::InvalidArchive);
    }
    let mut at = 1;
    let protected = bytes(encoded, &mut at, 38)?;
    if protected.len() != 38 || protected[..6] != [0xa2, 0x01, 0x32, 0x04, 0x58, 0x20] {
        return Err(Error::InvalidArchive);
    }
    let public = protected[6..]
        .try_into()
        .map_err(|_| Error::InvalidArchive)?;
    if encoded.get(at) != Some(&0xa0) {
        return Err(Error::InvalidArchive);
    }
    at += 1;
    bytes(encoded, &mut at, limits.max_manifest)?;
    if bytes(encoded, &mut at, 64)?.len() != 64 || at != encoded.len() {
        return Err(Error::InvalidArchive);
    }
    Ok(public)
}
pub fn verify(encoded: &[u8], expected: NodeId, limits: Limits) -> Result<SignedInventory> {
    let public = preflight(encoded, limits)?;
    let node = NodeId::from_public_key(&public).map_err(|_| Error::InvalidSignature)?;
    if node != expected {
        return Err(Error::WrongNode);
    }
    let cose = CoseSign1::from_slice(encoded).map_err(|_| Error::InvalidArchive)?;
    let payload = cose.payload.as_ref().ok_or(Error::InvalidArchive)?;
    let inventory: Inventory =
        serde_json::from_slice(payload).map_err(|_| Error::InvalidArchive)?;
    inventory.validate(limits)?;
    if inventory.node != node.to_string()
        || serde_json::to_vec(&inventory).map_err(|_| Error::InvalidArchive)? != *payload
    {
        return Err(Error::InvalidArchive);
    }
    let mut canonical = unsigned(payload.clone(), &public);
    canonical.signature = cose.signature.clone();
    if canonical.to_vec().map_err(|_| Error::InvalidArchive)? != encoded {
        return Err(Error::InvalidArchive);
    }
    let key = VerifyingKey::from_bytes(&public).map_err(|_| Error::InvalidSignature)?;
    cose.verify_signature(DOMAIN.as_bytes(), |signature, message| {
        key.verify_strict(
            message,
            &Signature::try_from(signature).map_err(|_| Error::InvalidSignature)?,
        )
        .map_err(|_| Error::InvalidSignature)
    })?;
    Ok(SignedInventory {
        encoded: encoded.to_vec(),
        inventory,
        id: Sha256::digest(encoded).into(),
        node,
    })
}
