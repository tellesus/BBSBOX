// SPDX-License-Identifier: AGPL-3.0-or-later
//! Client-held signing core. Native client distribution/trust and recovery UI remain separate gates.
use bbsbox_credentials::{
    ContactCard, Credential, DeviceBody, Error, PersonId, Purpose, Purposes, Result,
};
use ed25519_dalek::{Signer, SigningKey};
use rand_core::{OsRng, RngCore};
use zeroize::Zeroizing;
pub mod recovery;

fn generate() -> SigningKey {
    let mut seed = Zeroizing::new([0; 32]);
    OsRng.fill_bytes(seed.as_mut());
    SigningKey::from_bytes(&seed)
}
/// Client-held person root. No Debug, Clone or plaintext serialization interface.
pub struct RootKey(SigningKey);
pub struct DeviceKey(SigningKey);
pub struct ActiveDevice {
    key: SigningKey,
    credential: Credential,
}
impl Default for RootKey {
    fn default() -> Self {
        Self::generate()
    }
}
impl Default for DeviceKey {
    fn default() -> Self {
        Self::generate()
    }
}
impl RootKey {
    pub fn generate() -> Self {
        Self(generate())
    }
    pub fn public(&self) -> [u8; 32] {
        self.0.verifying_key().to_bytes()
    }
    pub fn person_id(&self) -> PersonId {
        PersonId::from_public_key(&self.public()).expect("generated valid root")
    }
    pub fn contact_card(&self) -> ContactCard {
        ContactCard::new(self.public()).expect("generated valid root")
    }
    pub fn prove_account_link(
        &self,
        challenge: &bbsbox_credentials::linking::Challenge,
        expected_node: bbsbox_credentials::NodeId,
        expected_account: &[u8; 16],
    ) -> Result<Vec<u8>> {
        challenge.check_destination(expected_node, expected_account)?;
        bbsbox_credentials::linking::prove(challenge, &self.contact_card(), |message| {
            self.0.sign(message).to_bytes()
        })
    }
    pub fn issue_device(&self, device_public: [u8; 32], purposes: Purposes) -> Result<Credential> {
        let mut incarnation = [0; 16];
        while incarnation == [0; 16] {
            OsRng.fill_bytes(&mut incarnation);
        }
        let body = DeviceBody::new(self.public(), device_public, incarnation, purposes)?;
        bbsbox_credentials::issue(body, |message| self.0.sign(message).to_bytes())
    }
}
impl DeviceKey {
    pub fn generate() -> Self {
        Self(generate())
    }
    pub fn public(&self) -> [u8; 32] {
        self.0.verifying_key().to_bytes()
    }
    pub fn activate(self, person: PersonId, credential: Credential) -> Result<ActiveDevice> {
        if credential.body().person_id() != person {
            return Err(Error::WrongPerson);
        }
        if credential.body().device() != &self.public() {
            return Err(Error::InvalidKey);
        }
        Ok(ActiveDevice {
            key: self.0,
            credential,
        })
    }
}
impl ActiveDevice {
    pub fn credential(&self) -> &Credential {
        &self.credential
    }
    /// Consumer supplies its domain-separated signed structure and performs resource authorization on receipt.
    pub fn sign_for(&self, purpose: Purpose, message: &[u8]) -> Result<[u8; 64]> {
        if !self.credential.body().purposes().contains(purpose) {
            return Err(Error::PurposeDenied);
        }
        Ok(self.key.sign(message).to_bytes())
    }
}
