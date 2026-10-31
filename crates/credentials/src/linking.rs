// SPDX-License-Identifier: AGPL-3.0-or-later
//! One-purpose root possession proof bound to a destination account, node and fresh challenge.
use crate::{ContactCard, Error, NodeId, PersonId, Result, encode, public_key};
use ciborium::value::Value;
use coset::{CborSerializable, CoseSign1, CoseSign1Builder, HeaderBuilder, iana};
use ed25519_dalek::Signature;
pub const DOMAIN: &str = "bbsbox.account-link/v1";
#[derive(Clone, Debug)]
pub struct Challenge {
    node: NodeId,
    account: [u8; 16],
    person: PersonId,
    nonce: [u8; 32],
}
impl Challenge {
    pub fn new(node: NodeId, account: [u8; 16], person: PersonId, nonce: [u8; 32]) -> Result<Self> {
        if nonce == [0; 32] {
            return Err(Error::Malformed);
        }
        Ok(Self {
            node,
            account,
            person,
            nonce,
        })
    }
    pub fn node(&self) -> NodeId {
        self.node
    }
    pub fn account(&self) -> &[u8; 16] {
        &self.account
    }
    pub fn person(&self) -> PersonId {
        self.person
    }
    pub fn nonce(&self) -> &[u8; 32] {
        &self.nonce
    }
    pub fn check_destination(&self, node: NodeId, account: &[u8; 16]) -> Result<()> {
        if self.node != node || self.account != *account {
            return Err(Error::WrongDestination);
        }
        Ok(())
    }
    pub fn encoded(&self) -> Result<Vec<u8>> {
        encode(&Value::Array(vec![
            Value::Text(DOMAIN.into()),
            Value::Integer(1.into()),
            Value::Bytes(self.node.bytes().to_vec()),
            Value::Bytes(self.account.to_vec()),
            Value::Bytes(self.person.bytes().to_vec()),
            Value::Bytes(self.nonce.to_vec()),
        ]))
    }
}
fn unsigned(challenge: &Challenge) -> Result<CoseSign1> {
    Ok(CoseSign1Builder::new()
        .protected(
            HeaderBuilder::new()
                .algorithm(iana::Algorithm::EdDSA)
                .key_id(challenge.person.bytes().to_vec())
                .build(),
        )
        .payload(challenge.encoded()?)
        .build())
}
pub fn prove(
    challenge: &Challenge,
    card: &ContactCard,
    sign: impl FnOnce(&[u8]) -> [u8; 64],
) -> Result<Vec<u8>> {
    card.verify_fingerprint(challenge.person)?;
    let unsigned = unsigned(challenge)?;
    let bytes = CoseSign1Builder::new()
        .protected(unsigned.protected.header)
        .payload(unsigned.payload.ok_or(Error::Malformed)?)
        .create_signature(DOMAIN.as_bytes(), |message| sign(message).to_vec())
        .build()
        .to_vec()
        .map_err(|_| Error::Malformed)?;
    verify(challenge, card, &bytes)?;
    Ok(bytes)
}
pub fn verify(challenge: &Challenge, card: &ContactCard, encoded: &[u8]) -> Result<()> {
    card.verify_fingerprint(challenge.person)?;
    if encoded.len() > 512 {
        return Err(Error::Malformed);
    }
    let cose = CoseSign1::from_slice(encoded).map_err(|_| Error::Malformed)?;
    let mut expected = unsigned(challenge)?;
    expected.signature = cose.signature.clone();
    if expected.to_vec().map_err(|_| Error::Malformed)? != encoded {
        return Err(Error::NonCanonical);
    }
    public_key(card.root())?
        .verify_strict(
            &cose.tbs_data(DOMAIN.as_bytes()),
            &Signature::try_from(cose.signature.as_slice()).map_err(|_| Error::InvalidSignature)?,
        )
        .map_err(|_| Error::InvalidSignature)
}
