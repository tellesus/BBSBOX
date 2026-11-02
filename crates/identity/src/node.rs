// SPDX-License-Identifier: AGPL-3.0-or-later
//! Atomic node-only key state; every runtime gets a fresh incarnation. M04 owns scoped counters/envelopes.
use crate::{Error, Result};
use bbsbox_credentials::NodeId;
use bbsbox_storage::{Database, disk::Class};
use ed25519_dalek::{Signer, SigningKey};
use rand_core::{OsRng, RngCore};
use rusqlite::{OptionalExtension, params};
use zeroize::Zeroizing;

pub struct NodeSigner {
    key: SigningKey,
    incarnation: [u8; 16],
    id: NodeId,
}
impl NodeSigner {
    /// Read-only source/snapshot inspection. This grants no signing or restore-activation authority.
    pub fn inspect_existing(path: impl AsRef<std::path::Path>) -> Result<NodeId> {
        let connection = bbsbox_storage::open_reader(path)?;
        let (public, raw): (Vec<u8>, Vec<u8>) = connection
            .query_row(
                "SELECT public_key,seed FROM identity_node_identity WHERE id=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or(Error::InvalidNodeState)?;
        let raw = Zeroizing::new(raw);
        let seed: Zeroizing<[u8; 32]> = Zeroizing::new(
            raw.as_slice()
                .try_into()
                .map_err(|_| Error::InvalidNodeState)?,
        );
        let key = SigningKey::from_bytes(&seed);
        let derived = key.verifying_key().to_bytes();
        if public.as_slice() != derived {
            return Err(Error::InvalidNodeState);
        }
        Ok(NodeId::from_public_key(&derived)?)
    }
    /// Use on the storage writer at startup. Restored storage remains blocked until M12 activation.
    pub fn start(database: &mut Database) -> Result<Self> {
        Self::load(database, None)
    }
    /// Maintenance must bind to an existing node; it must never initialize a replacement identity.
    pub fn start_existing(database: &mut Database, expected_node: NodeId) -> Result<Self> {
        Self::load(database, Some(expected_node))
    }
    fn load(database: &mut Database, expected_node: Option<NodeId>) -> Result<Self> {
        if database.recovery_required()? {
            return Err(bbsbox_storage::Error::RecoveryRequired.into());
        }
        database.write(Class::Control, 65536, |tx| {
            let known: Option<(Vec<u8>, Vec<u8>)> = tx
                .query_row(
                    "SELECT public_key,seed FROM identity_node_identity WHERE id=1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let mut seed = Zeroizing::new([0; 32]);
            let expected = if let Some((public, raw)) = known {
                let raw = Zeroizing::new(raw);
                if raw.len() != 32 {
                    return Err(Error::InvalidNodeState);
                }
                seed.copy_from_slice(&raw);
                Some(public)
            } else {
                if expected_node.is_some() {
                    return Err(Error::InvalidNodeState);
                }
                OsRng.fill_bytes(seed.as_mut());
                None
            };
            let key = SigningKey::from_bytes(&seed);
            let public = key.verifying_key().to_bytes();
            let id = NodeId::from_public_key(&public)?;
            if expected
                .as_ref()
                .is_some_and(|expected| expected.as_slice() != public)
            {
                return Err(Error::InvalidNodeState);
            }
            if expected_node.is_some_and(|expected| expected != id) {
                return Err(Error::InvalidNodeState);
            }
            if expected.is_none() {
                tx.execute(
                    "INSERT INTO identity_node_identity VALUES(1,?1,?2)",
                    params![public.as_slice(), seed.as_slice()],
                )?;
            }
            let mut incarnation = [0; 16];
            while incarnation == [0; 16] {
                OsRng.fill_bytes(&mut incarnation);
            }
            Ok(Self {
                key,
                incarnation,
                id,
            })
        })
    }
    pub fn id(&self) -> NodeId {
        self.id
    }
    pub fn public(&self) -> [u8; 32] {
        self.key.verifying_key().to_bytes()
    }
    pub fn incarnation(&self) -> &[u8; 16] {
        &self.incarnation
    }
    /// M04 builds the signed structure from this ID/incarnation and its resource scope/counter.
    pub fn sign_current(&self, incarnation: &[u8; 16], message: &[u8]) -> Result<[u8; 64]> {
        if incarnation != &self.incarnation {
            return Err(Error::StaleSigner);
        }
        Ok(self.key.sign(message).to_bytes())
    }
}
