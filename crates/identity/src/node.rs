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
    /// Use on the storage writer at startup. Restored storage remains blocked until M12 activation.
    pub fn start(database: &mut Database) -> Result<Self> {
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
                OsRng.fill_bytes(seed.as_mut());
                None
            };
            let key = SigningKey::from_bytes(&seed);
            let public = key.verifying_key().to_bytes();
            if expected
                .as_ref()
                .is_some_and(|expected| expected.as_slice() != public)
            {
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
                id: NodeId::from_public_key(&public)?,
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
