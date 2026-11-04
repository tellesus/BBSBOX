// SPDX-License-Identifier: AGPL-3.0-or-later
//! Foundation-node continuation in isolated storage. Installed selection/handover stays separate.
use crate::{
    Error, Limits, Result, archive, inventory,
    snapshot::{self, RestoredSnapshot},
};
use bbsbox_credentials::NodeId;
use bbsbox_identity::node::NodeSigner;
use bbsbox_storage::{Migration, recovery::Recovery};
use coset::{CborSerializable, CoseSign1};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const DOMAIN: &str = "bbsbox.recovery-decision/v1";
const MAX_DECISION: usize = 16384;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceDisposition {
    Retired,
    Lost,
}
#[derive(Clone, Debug)]
pub struct Authorization {
    pub source: SourceDisposition,
    pub reason: String,
    pub recorded_at: i64,
}
impl Authorization {
    fn valid(&self) -> bool {
        !self.reason.trim().is_empty()
            && self.reason.len() <= 512
            && !self.reason.chars().any(char::is_control)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub format: String,
    pub node: String,
    pub source_inventory_sha256: String,
    pub snapshot_time: i64,
    pub source_incarnation: [u8; 16],
    pub prepared_incarnation: [u8; 16],
    pub source: SourceDisposition,
    pub reason: String,
    pub recorded_at: i64,
    pub scope: String,
    pub module_owners: Vec<String>,
    pub post_snapshot_writes: String,
    pub client_recovery: String,
}
impl Decision {
    fn valid(&self, node: NodeId) -> bool {
        self.format == DOMAIN
            && self.node == node.to_string()
            && self.scope == "foundation-node"
            && self.source_inventory_sha256.len() == 64
            && self
                .source_inventory_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            && self.source_inventory_sha256.bytes().any(|b| b != b'0')
            && self.source_incarnation != [0; 16]
            && self.prepared_incarnation != [0; 16]
            && self.source_incarnation != self.prepared_incarnation
            && Authorization {
                source: self.source,
                reason: self.reason.clone(),
                recorded_at: self.recorded_at,
            }
            .valid()
            && !self.module_owners.is_empty()
            && self
                .module_owners
                .iter()
                .all(|m| crate::foundation_owner(m))
            && self.module_owners.windows(2).all(|m| m[0] < m[1])
            && self.post_snapshot_writes == "not-reconciled"
            && self.client_recovery == "separate"
    }
}
pub struct SignedDecision {
    encoded: Vec<u8>,
    decision: Decision,
    id: [u8; 32],
    node: NodeId,
}
impl SignedDecision {
    pub fn encoded(&self) -> &[u8] {
        &self.encoded
    }
    pub fn decision(&self) -> &Decision {
        &self.decision
    }
    pub fn id(&self) -> &[u8; 32] {
        &self.id
    }
    pub fn node(&self) -> NodeId {
        self.node
    }
}
fn sign(decision: Decision, signer: &NodeSigner) -> Result<SignedDecision> {
    if !decision.valid(signer.id()) || decision.prepared_incarnation != *signer.incarnation() {
        return Err(Error::InvalidContinuation);
    }
    let payload = serde_json::to_vec(&decision).map_err(|_| Error::InvalidContinuation)?;
    if payload.len() > MAX_DECISION {
        return Err(Error::LimitExceeded);
    }
    let mut cose = inventory::unsigned(payload, &signer.public());
    cose.signature = signer
        .sign_current(signer.incarnation(), &cose.tbs_data(DOMAIN.as_bytes()))?
        .to_vec();
    verify(
        &cose.to_vec().map_err(|_| Error::InvalidContinuation)?,
        signer.id(),
    )
}
pub fn verify(encoded: &[u8], expected: NodeId) -> Result<SignedDecision> {
    let key = inventory::preflight(
        encoded,
        Limits {
            max_manifest: MAX_DECISION,
            ..Limits::default()
        },
    )
    .map_err(|error| match error {
        Error::LimitExceeded => Error::LimitExceeded,
        _ => Error::InvalidContinuation,
    })?;
    if NodeId::from_public_key(&key).map_err(|_| Error::WrongNode)? != expected {
        return Err(Error::WrongNode);
    }
    let cose = CoseSign1::from_slice(encoded).map_err(|_| Error::InvalidContinuation)?;
    let payload = cose.payload.as_ref().ok_or(Error::InvalidContinuation)?;
    let decision: Decision =
        serde_json::from_slice(payload).map_err(|_| Error::InvalidContinuation)?;
    if !decision.valid(expected)
        || serde_json::to_vec(&decision).map_err(|_| Error::InvalidContinuation)? != *payload
    {
        return Err(Error::InvalidContinuation);
    }
    let mut canonical = inventory::unsigned(payload.clone(), &key);
    canonical.signature = cose.signature.clone();
    if canonical.to_vec().map_err(|_| Error::InvalidContinuation)? != encoded {
        return Err(Error::InvalidContinuation);
    }
    let key = VerifyingKey::from_bytes(&key).map_err(|_| Error::InvalidSignature)?;
    cose.verify_signature(DOMAIN.as_bytes(), |signature, message| {
        key.verify_strict(
            message,
            &Signature::try_from(signature).map_err(|_| Error::InvalidSignature)?,
        )
        .map_err(|_| Error::InvalidSignature)
    })?;
    Ok(SignedDecision {
        encoded: encoded.into(),
        decision,
        id: Sha256::digest(encoded).into(),
        node: expected,
    })
}
fn read(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(Error::UnsafePath);
    }
    let mut bytes = vec![];
    File::open(path)?
        .take((maximum + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err(Error::LimitExceeded);
    }
    Ok(bytes)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RestoreReport {
    format: String,
    node: String,
    inventory_sha256: String,
    snapshot_time: i64,
    state: String,
    post_snapshot_writes: String,
}

pub struct Prepared {
    recovery: Recovery,
    signed: SignedDecision,
    configuration: PathBuf,
    report_path: PathBuf,
    report: RestoreReport,
}
/// Core-only foundation owners. Future module/game consumers must
/// extend the profile with their own validation/effect fences before claiming complete continuation.
pub fn prepare(
    restored: &RestoredSnapshot,
    source: &archive::OpenedArchive,
    migrations: &[Migration],
    authorization: Authorization,
) -> Result<Prepared> {
    if !authorization.valid()
        || migrations.is_empty()
        || migrations
            .iter()
            .any(|m| !crate::foundation_owner(m.module))
    {
        return Err(Error::InvalidContinuation);
    }
    if restored
        .directory()
        .join(".restore-incomplete")
        .try_exists()?
    {
        return Err(Error::InvalidContinuation);
    }
    let descriptor = snapshot::verify(&source.content(), source.inventory(), migrations)?;
    // No configuration migration in this profile; only M12's current decision may change.
    let observed = archive::inventory(
        &restored.configuration(),
        Limits {
            max_files: 121,
            max_member_bytes: 1024 * 1024,
            max_total_bytes: 8 * 1024 * 1024 + MAX_DECISION as u64,
            ..Limits::default()
        },
    )?;
    let original = descriptor
        .configuration
        .iter()
        .filter(|r| r.path != "maintenance/recovery.cose")
        .collect::<Vec<_>>();
    let current = observed
        .iter()
        .filter(|r| r.path != "maintenance/recovery.cose")
        .collect::<Vec<_>>();
    if current != original {
        return Err(Error::InvalidContinuation);
    }
    let report: RestoreReport =
        serde_json::from_slice(&read(&restored.directory().join("recovery.json"), 65536)?)
            .map_err(|_| Error::InvalidContinuation)?;
    let inventory = source.inventory();
    if report.format != "bbsbox.isolated-node-restore/v1"
        || report.node != descriptor.node
        || report.inventory_sha256 != hex::encode(inventory.id())
        || report.snapshot_time != descriptor.created_at
        || !matches!(
            report.state.as_str(),
            "recovery-required" | "continuation-authorized"
        )
        || report.post_snapshot_writes != "not-reconciled"
    {
        return Err(Error::InvalidContinuation);
    }
    if report.state == "continuation-authorized" {
        let previous = verify(
            &read(
                &restored.configuration().join("maintenance/recovery.cose"),
                MAX_DECISION + 128,
            )?,
            inventory.node(),
        )?;
        if previous.decision().source_inventory_sha256 != report.inventory_sha256
            || previous.decision().snapshot_time != report.snapshot_time
        {
            return Err(Error::InvalidContinuation);
        }
    }
    let mut recovery = Recovery::open(restored.database(), migrations)?;
    let signer = NodeSigner::prepare_recovery(&mut recovery, inventory.node())?;
    let decision = Decision {
        format: DOMAIN.into(),
        node: inventory.node().to_string(),
        source_inventory_sha256: hex::encode(inventory.id()),
        snapshot_time: descriptor.created_at,
        source_incarnation: inventory.inventory().incarnation,
        prepared_incarnation: *signer.incarnation(),
        source: authorization.source,
        reason: authorization.reason,
        recorded_at: authorization.recorded_at,
        scope: "foundation-node".into(),
        module_owners: migrations
            .iter()
            .map(|m| m.module.to_owned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        post_snapshot_writes: "not-reconciled".into(),
        client_recovery: "separate".into(),
    };
    let signed = sign(decision, &signer)?;
    Ok(Prepared {
        recovery,
        signed,
        configuration: restored.configuration(),
        report_path: restored.directory().join("recovery.json"),
        report,
    })
}
impl Prepared {
    pub fn decision(&self) -> &SignedDecision {
        &self.signed
    }
    /// Save the protected current decision before releasing only this isolated writer fence.
    /// Does not move data/configuration, switch an installed service or recover client keys.
    pub fn finish_isolated(mut self) -> Result<SignedDecision> {
        for parent in std::path::absolute(&self.configuration)?.ancestors() {
            let metadata = fs::symlink_metadata(parent)?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err(Error::UnsafePath);
            }
        }
        let directory = self.configuration.join("maintenance");
        match fs::symlink_metadata(&directory) {
            Ok(metadata) => {
                if !metadata.is_dir() || metadata.file_type().is_symlink() {
                    return Err(Error::UnsafePath);
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if metadata.permissions().mode() & 0o077 != 0 {
                        return Err(Error::UnsafePath);
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                archive::private_directory(&directory)?
            }
            Err(error) => return Err(error.into()),
        }
        let record = directory.join("recovery.cose");
        match fs::symlink_metadata(&record) {
            Ok(_) => {
                verify(&read(&record, MAX_DECISION + 128)?, self.signed.node())?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
        archive::space(&directory, self.signed.encoded().len() as u64)?;
        let mut temporary = tempfile::NamedTempFile::new_in(&directory)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if temporary.as_file().metadata()?.permissions().mode() & 0o077 != 0 {
                return Err(Error::UnsafePath);
            }
        }
        temporary.write_all(self.signed.encoded())?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(&record)
            .map_err(|error| Error::Io(error.error))?;
        archive::sync_directory(&directory)?;
        self.report.state = "continuation-authorized".into();
        let mut report =
            tempfile::NamedTempFile::new_in(self.report_path.parent().ok_or(Error::UnsafePath)?)?;
        report.write_all(
            &serde_json::to_vec(&self.report).map_err(|_| Error::InvalidContinuation)?,
        )?;
        report.as_file().sync_all()?;
        report
            .persist(&self.report_path)
            .map_err(|error| Error::Io(error.error))?;
        archive::sync_directory(self.report_path.parent().ok_or(Error::UnsafePath)?)?;
        self.recovery
            .finish_after_decision(&record, *self.signed.id())?;
        Ok(self.signed)
    }
}
