// SPDX-License-Identifier: AGPL-3.0-or-later
//! User-held root/client-state export. Restoring never reconstructs an old active signing device.
use crate::{ActiveDevice, DeviceKey, RootKey};
use argon2::{Algorithm, Argon2, Params, Version};
use bbsbox_credentials::{ContactCard, PersonId, Purposes};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use ciborium::value::Value;
use ed25519_dalek::SigningKey;
use rand_core::{OsRng, RngCore};
use std::{
    fmt,
    sync::atomic::{AtomicBool, Ordering},
};
use zeroize::Zeroizing;

pub const MAX_CLIENT_STATE: usize = 8 * 1024 * 1024;
const MAX_KIT: usize = MAX_CLIENT_STATE + 256;
const DOMAIN: &str = "bbsbox.recovery-kit/v1";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidKit,
    UnsupportedProfile,
    WeakPassphrase,
    DecryptFailed,
    Busy,
    Credential(bbsbox_credentials::Error),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidKit => "invalid or excessive recovery kit",
            Self::UnsupportedProfile => "unsupported recovery format or KDF profile",
            Self::WeakPassphrase => {
                "recovery passphrase must contain at least 15 characters and at most 1024 bytes"
            }
            Self::DecryptFailed => "recovery passphrase or kit authentication is invalid",
            Self::Busy => "a recovery operation is already running",
            Self::Credential(_) => "fresh device credential could not be created",
        })
    }
}
impl std::error::Error for Error {}
impl From<bbsbox_credentials::Error> for Error {
    fn from(e: bbsbox_credentials::Error) -> Self {
        Self::Credential(e)
    }
}
type Result<T> = std::result::Result<T, Error>;
#[derive(Default)]
pub struct Engine {
    busy: AtomicBool,
}
struct Permit<'a>(&'a AtomicBool);
impl Drop for Permit<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
/// Root plus opaque receive-side client state; exposes no previous active device or counter state as a signer.
pub struct RestoredIdentity {
    root: RootKey,
    state: Zeroizing<Vec<u8>>,
}
impl RestoredIdentity {
    pub fn person_id(&self) -> PersonId {
        self.root.person_id()
    }
    pub fn contact_card(&self) -> ContactCard {
        self.root.contact_card()
    }
    pub fn prove_account_link(
        &self,
        challenge: &bbsbox_credentials::linking::Challenge,
        expected_node: bbsbox_credentials::NodeId,
        expected_account: &[u8; 16],
    ) -> Result<Vec<u8>> {
        Ok(self
            .root
            .prove_account_link(challenge, expected_node, expected_account)?)
    }
    pub fn client_state(&self) -> &[u8] {
        &self.state
    }
    /// M13 must independently obtain a fresh owner-authorized MLS join before sending private group messages.
    pub fn new_device(&self, purposes: Purposes) -> Result<ActiveDevice> {
        let device = DeviceKey::generate();
        let credential = self.root.issue_device(device.public(), purposes)?;
        Ok(device.activate(self.root.person_id(), credential)?)
    }
    pub fn reexport(&self, engine: &Engine, passphrase: Zeroizing<String>) -> Result<Vec<u8>> {
        engine.export(&self.root, passphrase, &self.state)
    }
}
fn encode(value: &Value) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    ciborium::ser::into_writer(value, &mut bytes).map_err(|_| Error::InvalidKit)?;
    Ok(bytes)
}
fn header(salt: [u8; 16], nonce: [u8; 24]) -> Vec<Value> {
    vec![
        Value::Text(DOMAIN.into()),
        Value::Integer(1.into()),
        Value::Integer(1.into()),
        Value::Bytes(salt.to_vec()),
        Value::Bytes(nonce.to_vec()),
    ]
}
fn passphrase(value: &str) -> Result<()> {
    if value.chars().count() < 15 || value.len() > 1024 {
        return Err(Error::WeakPassphrase);
    }
    Ok(())
}
fn derive(value: &str, salt: &[u8; 16]) -> Result<Zeroizing<[u8; 32]>> {
    let mut key = Zeroizing::new([0; 32]);
    let params = Params::new(65536, 3, 1, Some(32)).map_err(|_| Error::UnsupportedProfile)?;
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(value.as_bytes(), salt, key.as_mut())
        .map_err(|_| Error::InvalidKit)?;
    Ok(key)
}
impl Engine {
    fn permit(&self) -> Result<Permit<'_>> {
        self.busy
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .map_err(|_| Error::Busy)?;
        Ok(Permit(&self.busy))
    }
    /// Run on the client's blocking recovery worker. The only adjustable input is bounded private state.
    pub fn export(
        &self,
        root: &RootKey,
        pass: Zeroizing<String>,
        client_state: &[u8],
    ) -> Result<Vec<u8>> {
        passphrase(&pass)?;
        if client_state.len() > MAX_CLIENT_STATE {
            return Err(Error::InvalidKit);
        }
        let _permit = self.permit()?;
        let mut salt = [0; 16];
        let mut nonce = [0; 24];
        OsRng.fill_bytes(&mut salt);
        OsRng.fill_bytes(&mut nonce);
        let mut fields = header(salt, nonce);
        let aad = encode(&Value::Array(fields.clone()))?;
        let key = derive(&pass, &salt)?;
        let cipher =
            XChaCha20Poly1305::new_from_slice(key.as_slice()).map_err(|_| Error::InvalidKit)?;
        let seed = Zeroizing::new(root.0.to_bytes());
        let mut plaintext = Zeroizing::new(Vec::with_capacity(32 + client_state.len()));
        plaintext.extend_from_slice(seed.as_slice());
        plaintext.extend_from_slice(client_state);
        let encrypted = cipher
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: &plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| Error::InvalidKit)?;
        fields.push(Value::Bytes(encrypted));
        encode(&Value::Array(fields))
    }
    pub fn import(&self, encoded: &[u8], pass: Zeroizing<String>) -> Result<RestoredIdentity> {
        passphrase(&pass)?;
        if encoded.len() > MAX_KIT {
            return Err(Error::InvalidKit);
        }
        let value: Value = ciborium::de::from_reader_with_recursion_limit(encoded, 4)
            .map_err(|_| Error::InvalidKit)?;
        let fields = value.as_array().ok_or(Error::InvalidKit)?;
        if fields.len() != 6 || fields[0].as_text() != Some(DOMAIN) {
            return Err(Error::InvalidKit);
        }
        if fields[1].as_integer().and_then(|n| u8::try_from(n).ok()) != Some(1)
            || fields[2].as_integer().and_then(|n| u8::try_from(n).ok()) != Some(1)
        {
            return Err(Error::UnsupportedProfile);
        }
        let salt: [u8; 16] = fields[3]
            .as_bytes()
            .ok_or(Error::InvalidKit)?
            .as_slice()
            .try_into()
            .map_err(|_| Error::InvalidKit)?;
        let nonce: [u8; 24] = fields[4]
            .as_bytes()
            .ok_or(Error::InvalidKit)?
            .as_slice()
            .try_into()
            .map_err(|_| Error::InvalidKit)?;
        let ciphertext = fields[5].as_bytes().ok_or(Error::InvalidKit)?;
        if ciphertext.len() < 48 || ciphertext.len() > MAX_CLIENT_STATE + 48 {
            return Err(Error::InvalidKit);
        }
        if encode(&value)? != encoded {
            return Err(Error::InvalidKit);
        }
        // Profile/size/canonical checks happen before acquiring the expensive fixed-cost KDF slot.
        let _permit = self.permit()?;
        let aad = encode(&Value::Array(header(salt, nonce)))?;
        let key = derive(&pass, &salt)?;
        let cipher =
            XChaCha20Poly1305::new_from_slice(key.as_slice()).map_err(|_| Error::InvalidKit)?;
        let plaintext = Zeroizing::new(
            cipher
                .decrypt(
                    &XNonce::from(nonce),
                    Payload {
                        msg: ciphertext,
                        aad: &aad,
                    },
                )
                .map_err(|_| Error::DecryptFailed)?,
        );
        if plaintext.len() < 32 || plaintext.len() > MAX_CLIENT_STATE + 32 {
            return Err(Error::InvalidKit);
        }
        let mut seed = Zeroizing::new([0; 32]);
        seed.copy_from_slice(&plaintext[..32]);
        Ok(RestoredIdentity {
            root: RootKey(SigningKey::from_bytes(&seed)),
            state: Zeroizing::new(plaintext[32..].to_vec()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_busy_recovery_worker_rejects_immediately_and_releases_its_slot() {
        let engine = Engine::default();
        let root = RootKey::generate();
        let permit = engine.permit().unwrap();
        assert!(matches!(
            engine.export(
                &root,
                Zeroizing::new("full recovery passphrase".into()),
                b"state"
            ),
            Err(Error::Busy)
        ));
        assert!(matches!(
            engine.import(
                b"invalid",
                Zeroizing::new("full recovery passphrase".into())
            ),
            Err(Error::InvalidKit)
        ));
        drop(permit);
        assert!(engine.permit().is_ok());
    }
}
