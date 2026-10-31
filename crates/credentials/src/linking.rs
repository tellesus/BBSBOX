// SPDX-License-Identifier: AGPL-3.0-or-later
//! One-purpose root possession proof bound to a destination account, node and fresh challenge.
use crate::{ContactCard, Error, NodeId, PersonId, Result, encode, public_key};
use ciborium::value::Value;
use coset::{CborSerializable, CoseSign1, CoseSign1Builder, HeaderBuilder};
use ed25519_dalek::Signature;
pub const DOMAIN: &str = "bbsbox.account-link/v2";
fn domain(version: u8) -> &'static str {
    if version == 1 {
        "bbsbox.account-link/v1"
    } else {
        DOMAIN
    }
}
#[derive(Clone, Debug)]
pub struct Challenge {
    version: u8,
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
            version: 2,
            node,
            account,
            person,
            nonce,
        })
    }
    pub fn node(&self) -> NodeId {
        self.node
    }
    pub fn version(&self) -> u8 {
        self.version
    }
    pub fn from_encoded(encoded: &[u8]) -> Result<Self> {
        if encoded.len() > 512 {
            return Err(Error::Malformed);
        }
        let value: Value = ciborium::de::from_reader_with_recursion_limit(encoded, 4)
            .map_err(|_| Error::Malformed)?;
        let fields = value.as_array().ok_or(Error::Malformed)?;
        if fields.len() != 6 {
            return Err(Error::Malformed);
        }
        let version = super::version(&fields[1])?;
        if fields[0].as_text() != Some(domain(version)) {
            return Err(Error::Malformed);
        }
        let mut challenge = Self::new(
            NodeId::from_digest(super::bytes(&fields[2])?),
            super::bytes(&fields[3])?,
            PersonId::from_digest(super::bytes(&fields[4])?),
            super::bytes(&fields[5])?,
        )?;
        challenge.version = version;
        if challenge.encoded()? != encoded {
            return Err(Error::NonCanonical);
        }
        Ok(challenge)
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
            Value::Text(domain(self.version).into()),
            Value::Integer(self.version.into()),
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
                .algorithm(super::algorithm(challenge.version))
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
    if challenge.version != 2 {
        return Err(Error::UnsupportedVersion);
    }
    card.verify_fingerprint(challenge.person)?;
    let unsigned = unsigned(challenge)?;
    let bytes = CoseSign1Builder::new()
        .protected(unsigned.protected.header)
        .payload(unsigned.payload.ok_or(Error::Malformed)?)
        .create_signature(domain(challenge.version).as_bytes(), |message| {
            sign(message).to_vec()
        })
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
            &cose.tbs_data(domain(challenge.version).as_bytes()),
            &Signature::try_from(cose.signature.as_slice()).map_err(|_| Error::InvalidSignature)?,
        )
        .map_err(|_| Error::InvalidSignature)
}
