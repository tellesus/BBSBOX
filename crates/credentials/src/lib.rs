// SPDX-License-Identifier: AGPL-3.0-or-later
//! Public identity definitions and root-authorized credentials. A verified signature is not resource permission.
use ciborium::value::Value;
use coset::{CborSerializable, CoseSign1, CoseSign1Builder, HeaderBuilder, iana};
use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};
use std::{fmt, str::FromStr};
pub mod linking;

pub const DEVICE_DOMAIN: &str = "bbsbox.device-credential/v1";
pub const MAX_CREDENTIAL_BYTES: usize = 512;
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Malformed,
    NonCanonical,
    UnsupportedVersion,
    UnsupportedPurpose,
    InvalidKey,
    SameRootAndDevice,
    InvalidSignature,
    WrongPerson,
    PurposeDenied,
    WrongDestination,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Malformed => "malformed public credential",
            Self::NonCanonical => "credential encoding is not canonical",
            Self::UnsupportedVersion => "unsupported credential version",
            Self::UnsupportedPurpose => "unsupported or empty device purposes",
            Self::InvalidKey => "invalid or weak public key",
            Self::SameRootAndDevice => "root and device keys must be separate",
            Self::InvalidSignature => "invalid credential signature",
            Self::WrongPerson => "credential belongs to another person",
            Self::PurposeDenied => "device credential does not authorize this purpose",
            Self::WrongDestination => "account-link challenge belongs to another destination",
        })
    }
}
impl std::error::Error for Error {}
fn public_key(bytes: &[u8; 32]) -> Result<VerifyingKey> {
    let key = VerifyingKey::from_bytes(bytes).map_err(|_| Error::InvalidKey)?;
    if key.is_weak() {
        return Err(Error::InvalidKey);
    }
    Ok(key)
}
macro_rules! identity_id {
    ($name:ident,$prefix:literal,$domain:literal) => {
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name([u8; 32]);
        impl $name {
            pub fn from_public_key(public: &[u8; 32]) -> Result<Self> {
                public_key(public)?;
                let mut hash = Sha256::new();
                hash.update($domain);
                hash.update(public);
                Ok(Self(hash.finalize().into()))
            }
            pub fn bytes(&self) -> &[u8; 32] {
                &self.0
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}:{}", $prefix, hex::encode(self.0))
            }
        }
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(self, f)
            }
        }
        impl FromStr for $name {
            type Err = Error;
            fn from_str(value: &str) -> Result<Self> {
                let encoded = value
                    .strip_prefix(concat!($prefix, ":"))
                    .ok_or(Error::Malformed)?;
                if encoded.len() != 64
                    || !encoded
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                {
                    return Err(Error::Malformed);
                }
                let bytes = hex::decode(encoded).map_err(|_| Error::Malformed)?;
                Ok(Self(bytes.try_into().map_err(|_| Error::Malformed)?))
            }
        }
    };
}
identity_id!(PersonId, "person", b"bbsbox.person/v1\0");
identity_id!(DeviceId, "device", b"bbsbox.device/v1\0");
identity_id!(NodeId, "node", b"bbsbox.node/v1\0");
identity_id!(ForumId, "forum", b"bbsbox.forum/v1\0");
identity_id!(CourierId, "courier", b"bbsbox.courier/v1\0");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Purpose {
    Authorship = 1,
    ForumGovernance = 2,
    GameOwnership = 4,
    IdentityProof = 8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Purposes(u8);
impl Purposes {
    pub fn new(purposes: &[Purpose]) -> Result<Self> {
        Self::from_bits(purposes.iter().fold(0, |bits, p| bits | *p as u8))
    }
    pub fn from_bits(bits: u8) -> Result<Self> {
        if bits == 0 || bits & !15 != 0 {
            return Err(Error::UnsupportedPurpose);
        }
        Ok(Self(bits))
    }
    pub fn bits(self) -> u8 {
        self.0
    }
    pub fn contains(self, purpose: Purpose) -> bool {
        self.0 & purpose as u8 != 0
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceBody {
    root: [u8; 32],
    device: [u8; 32],
    incarnation: [u8; 16],
    purposes: Purposes,
}
impl DeviceBody {
    pub fn new(
        root: [u8; 32],
        device: [u8; 32],
        incarnation: [u8; 16],
        purposes: Purposes,
    ) -> Result<Self> {
        public_key(&root)?;
        public_key(&device)?;
        if root == device {
            return Err(Error::SameRootAndDevice);
        }
        if incarnation == [0; 16] {
            return Err(Error::Malformed);
        }
        Ok(Self {
            root,
            device,
            incarnation,
            purposes,
        })
    }
    pub fn root(&self) -> &[u8; 32] {
        &self.root
    }
    pub fn device(&self) -> &[u8; 32] {
        &self.device
    }
    pub fn incarnation(&self) -> &[u8; 16] {
        &self.incarnation
    }
    pub fn purposes(&self) -> Purposes {
        self.purposes
    }
    pub fn person_id(&self) -> PersonId {
        PersonId::from_public_key(&self.root).expect("validated root")
    }
    pub fn device_id(&self) -> DeviceId {
        DeviceId::from_public_key(&self.device).expect("validated device")
    }
    fn payload(&self) -> Result<Vec<u8>> {
        encode(&Value::Array(vec![
            Value::Text(DEVICE_DOMAIN.into()),
            Value::Integer(1.into()),
            Value::Bytes(self.root.to_vec()),
            Value::Bytes(self.device.to_vec()),
            Value::Bytes(self.incarnation.to_vec()),
            Value::Integer(self.purposes.0.into()),
        ]))
    }
    fn parse(payload: &[u8]) -> Result<Self> {
        let value: Value = ciborium::de::from_reader(payload).map_err(|_| Error::Malformed)?;
        let fields = value.as_array().ok_or(Error::Malformed)?;
        if fields.len() != 6 || fields[0].as_text() != Some(DEVICE_DOMAIN) {
            return Err(Error::Malformed);
        }
        version(&fields[1])?;
        let body = Self::new(
            bytes(&fields[2])?,
            bytes(&fields[3])?,
            bytes(&fields[4])?,
            Purposes::from_bits(
                u8::try_from(fields[5].as_integer().ok_or(Error::Malformed)?)
                    .map_err(|_| Error::UnsupportedPurpose)?,
            )?,
        )?;
        if body.payload()? != payload {
            return Err(Error::NonCanonical);
        }
        Ok(body)
    }
}
fn encode(value: &Value) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    ciborium::ser::into_writer(value, &mut bytes).map_err(|_| Error::Malformed)?;
    Ok(bytes)
}
fn bytes<const N: usize>(value: &Value) -> Result<[u8; N]> {
    value
        .as_bytes()
        .ok_or(Error::Malformed)?
        .as_slice()
        .try_into()
        .map_err(|_| Error::Malformed)
}
fn version(value: &Value) -> Result<()> {
    if value.as_integer().and_then(|n| u64::try_from(n).ok()) != Some(1) {
        return Err(Error::UnsupportedVersion);
    }
    Ok(())
}
fn unsigned(body: &DeviceBody) -> Result<CoseSign1> {
    Ok(CoseSign1Builder::new()
        .protected(
            HeaderBuilder::new()
                .algorithm(iana::Algorithm::EdDSA)
                .key_id(body.person_id().bytes().to_vec())
                .build(),
        )
        .payload(body.payload()?)
        .build())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CredentialId([u8; 32]);
impl CredentialId {
    pub fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
}
impl fmt::Display for CredentialId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "credential:{}", hex::encode(self.0))
    }
}
/// Verifies root delegation only. Consumers must also resolve revocation/causal state and resource authority.
#[derive(Clone, Debug)]
pub struct Credential {
    encoded: Vec<u8>,
    body: DeviceBody,
    id: CredentialId,
}
impl Credential {
    pub fn encoded(&self) -> &[u8] {
        &self.encoded
    }
    pub fn body(&self) -> &DeviceBody {
        &self.body
    }
    pub fn id(&self) -> CredentialId {
        self.id
    }
    pub fn check_subject_purpose(&self, person: PersonId, purpose: Purpose) -> Result<()> {
        if self.body.person_id() != person {
            return Err(Error::WrongPerson);
        }
        if !self.body.purposes.contains(purpose) {
            return Err(Error::PurposeDenied);
        }
        Ok(())
    }
    pub fn verify(encoded: &[u8]) -> Result<Self> {
        if encoded.len() > MAX_CREDENTIAL_BYTES {
            return Err(Error::Malformed);
        }
        let cose = CoseSign1::from_slice(encoded).map_err(|_| Error::Malformed)?;
        let body = DeviceBody::parse(cose.payload.as_ref().ok_or(Error::Malformed)?)?;
        let mut canonical = unsigned(&body)?;
        canonical.signature = cose.signature.clone();
        // Rebuilding also canonicalizes protected bytes; unknown/unprotected fields and trailing data fail.
        if canonical.to_vec().map_err(|_| Error::Malformed)? != encoded {
            return Err(Error::NonCanonical);
        }
        let key = public_key(body.root())?;
        cose.verify_signature(DEVICE_DOMAIN.as_bytes(), |signature, message| {
            let signature = Signature::try_from(signature).map_err(|_| Error::InvalidSignature)?;
            key.verify_strict(message, &signature)
                .map_err(|_| Error::InvalidSignature)
        })?;
        Ok(Self {
            encoded: encoded.into(),
            body,
            id: CredentialId(Sha256::digest(encoded).into()),
        })
    }
}
/// Signing callback belongs to the client root-key holder. This crate stores no private material.
pub fn issue(body: DeviceBody, sign: impl FnOnce(&[u8]) -> [u8; 64]) -> Result<Credential> {
    let unsigned = unsigned(&body)?;
    let signed = CoseSign1Builder::new()
        .protected(unsigned.protected.header)
        .payload(unsigned.payload.ok_or(Error::Malformed)?)
        .create_signature(DEVICE_DOMAIN.as_bytes(), |message| sign(message).to_vec())
        .build()
        .to_vec()
        .map_err(|_| Error::Malformed)?;
    Credential::verify(&signed)
}

#[derive(Clone, Debug)]
pub struct ContactCard {
    root: [u8; 32],
}
impl ContactCard {
    pub fn new(root: [u8; 32]) -> Result<Self> {
        public_key(&root)?;
        Ok(Self { root })
    }
    pub fn root(&self) -> &[u8; 32] {
        &self.root
    }
    pub fn person_id(&self) -> PersonId {
        PersonId::from_public_key(&self.root).expect("validated card root")
    }
    pub fn verify_fingerprint(&self, expected: PersonId) -> Result<()> {
        if self.person_id() != expected {
            return Err(Error::WrongPerson);
        }
        Ok(())
    }
    pub fn qr_text(&self) -> String {
        format!("bbsbox-person-v1:{}", hex::encode(self.root))
    }
    pub fn from_qr_text(text: &str) -> Result<Self> {
        let raw = text
            .strip_prefix("bbsbox-person-v1:")
            .ok_or(Error::Malformed)?;
        if raw.len() != 64
            || !raw
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Error::Malformed);
        }
        Self::new(
            hex::decode(raw)
                .map_err(|_| Error::Malformed)?
                .try_into()
                .map_err(|_| Error::Malformed)?,
        )
    }
}
