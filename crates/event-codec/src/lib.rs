// SPDX-License-Identifier: AGPL-3.0-or-later
//! M04's transport-neutral signed-byte profile. Signatures, bodies and action authority remain distinct checks.
use bbsbox_credentials::{Credential, CredentialId, DeviceId, ForumId, NodeId, PersonId};
use ciborium::value::Value;
use coset::{CborSerializable, CoseSign1, CoseSign1Builder, HeaderBuilder, iana};
use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};
use std::fmt;
pub mod authority;
pub const DOMAIN: &str = "bbsbox.event/v1";
pub const MAX_ENVELOPE: usize = 4096;
pub const MAX_BODY: usize = 65536;
pub const MAX_DEPENDENCIES: usize = 32;
pub const MAX_EXTENSIONS: usize = 16;
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Malformed,
    NonCanonical,
    EnvelopeTooLarge,
    BodyTooLarge,
    BodyMismatch,
    InvalidField,
    InvalidSequence,
    WrongStream,
    InvalidKey,
    InvalidSignature,
    CredentialMismatch,
    MissingCredential(CredentialId),
    UnsupportedMajor(u16),
    UnsupportedMinor(u16),
    UnsupportedStructure,
    UnsupportedDomain,
    UnsupportedRole(u8),
    SignerUnavailable,
    ResolverUnavailable,
    UnsupportedDependency(u8),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "event validation failed: {self:?}")
    }
}
impl std::error::Error for Error {}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventId([u8; 32]);
impl EventId {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    pub fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
}
impl fmt::Display for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "event:{}", hex::encode(self.0))
    }
}
impl fmt::Debug for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum DependencyKind {
    Event = 1,
    Credential = 2,
    Blob = 3,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Dependency {
    pub kind: DependencyKind,
    pub digest: [u8; 32],
}
impl Dependency {
    pub fn event(id: EventId) -> Self {
        Self {
            kind: DependencyKind::Event,
            digest: *id.bytes(),
        }
    }
    pub fn credential(id: CredentialId) -> Self {
        Self {
            kind: DependencyKind::Credential,
            digest: *id.bytes(),
        }
    }
    pub fn blob(digest: [u8; 32]) -> Self {
        Self {
            kind: DependencyKind::Blob,
            digest,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Role {
    PersonDevice = 1,
    Node = 2,
    ForumRoot = 3,
    PersonRoot = 4,
}
impl Role {
    fn parse(value: u8) -> Result<Self> {
        match value {
            1 => Ok(Self::PersonDevice),
            2 => Ok(Self::Node),
            3 => Ok(Self::ForumRoot),
            4 => Ok(Self::PersonRoot),
            other => Err(Error::UnsupportedRole(other)),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Distribution {
    Local = 0,
    Shared = 1,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scope {
    pub resource: String,
    pub audience: String,
    pub distribution: Distribution,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Extension {
    pub id: u16,
    pub required: bool,
    pub data: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Actor {
    pub role: Role,
    pub subject: [u8; 32],
    pub key: [u8; 32],
    pub credential: Option<CredentialId>,
    pub incarnation: [u8; 16],
}
impl Actor {
    pub fn device(credential: &Credential) -> Self {
        Self {
            role: Role::PersonDevice,
            subject: *credential.body().person_id().bytes(),
            key: *credential.body().device(),
            credential: Some(credential.id()),
            incarnation: *credential.body().incarnation(),
        }
    }
    pub fn root(role: Role, key: [u8; 32], incarnation: [u8; 16]) -> Result<Self> {
        let subject = match role {
            Role::Node => *NodeId::from_public_key(&key)
                .map_err(|_| Error::InvalidKey)?
                .bytes(),
            Role::ForumRoot => *ForumId::from_public_key(&key)
                .map_err(|_| Error::InvalidKey)?
                .bytes(),
            Role::PersonRoot => *PersonId::from_public_key(&key)
                .map_err(|_| Error::InvalidKey)?
                .bytes(),
            Role::PersonDevice => return Err(Error::CredentialMismatch),
        };
        if incarnation == [0; 16] {
            return Err(Error::InvalidField);
        }
        Ok(Self {
            role,
            subject,
            key,
            credential: None,
            incarnation,
        })
    }
    pub fn device_id(&self) -> Result<DeviceId> {
        if self.role != Role::PersonDevice {
            return Err(Error::CredentialMismatch);
        }
        DeviceId::from_public_key(&self.key).map_err(|_| Error::InvalidKey)
    }
}
#[derive(Clone, Debug)]
pub struct Draft {
    pub kind: String,
    pub schema: u16,
    pub scope: Scope,
    pub sequence: u64,
    pub previous: Option<EventId>,
    pub dependencies: Vec<Dependency>,
    pub claimed_time: i64,
    pub extensions: Vec<Extension>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Metadata {
    pub major: u16,
    pub minor: u16,
    pub kind: String,
    pub schema: u16,
    pub actor: Actor,
    pub stream: [u8; 32],
    pub sequence: u64,
    pub previous: Option<EventId>,
    pub scope: Scope,
    pub dependencies: Vec<Dependency>,
    pub claimed_time: i64,
    pub body_hash: [u8; 32],
    pub body_length: u32,
    pub extensions: Vec<Extension>,
}
fn token(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-.:/".contains(&b))
}
fn encode(value: &Value) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    ciborium::ser::into_writer(value, &mut out).map_err(|_| Error::Malformed)?;
    Ok(out)
}
fn binary(bytes: &[u8]) -> Value {
    Value::Bytes(bytes.to_vec())
}
fn integer(value: u64) -> Value {
    Value::Integer(value.into())
}
fn optional(value: Option<&[u8]>) -> Value {
    value.map(binary).unwrap_or(Value::Null)
}
pub fn stream_id(actor: &Actor, scope: &Scope) -> Result<[u8; 32]> {
    let scope = Value::Array(vec![
        Value::Text("bbsbox.stream/v1".into()),
        integer(actor.role as u64),
        binary(&actor.subject),
        binary(&actor.key),
        binary(&actor.incarnation),
        Value::Text(scope.resource.clone()),
        Value::Text(scope.audience.clone()),
        integer(scope.distribution as u64),
    ]);
    Ok(Sha256::digest(encode(&scope)?).into())
}
impl Metadata {
    fn payload(&self) -> Result<Vec<u8>> {
        let actor = &self.actor;
        encode(&Value::Array(vec![
            Value::Text(DOMAIN.into()),
            integer(self.major as u64),
            integer(self.minor as u64),
            Value::Text(self.kind.clone()),
            integer(self.schema as u64),
            Value::Array(vec![
                integer(actor.role as u64),
                binary(&actor.subject),
                binary(&actor.key),
                optional(actor.credential.as_ref().map(|id| id.bytes().as_slice())),
                binary(&actor.incarnation),
            ]),
            binary(&self.stream),
            integer(self.sequence),
            optional(self.previous.as_ref().map(|id| id.bytes().as_slice())),
            Value::Text(self.scope.resource.clone()),
            Value::Text(self.scope.audience.clone()),
            integer(self.scope.distribution as u64),
            Value::Array(
                self.dependencies
                    .iter()
                    .map(|dep| Value::Array(vec![integer(dep.kind as u64), binary(&dep.digest)]))
                    .collect(),
            ),
            Value::Integer(self.claimed_time.into()),
            binary(&self.body_hash),
            integer(self.body_length as u64),
            Value::Array(
                self.extensions
                    .iter()
                    .map(|e| {
                        Value::Array(vec![
                            integer(e.id as u64),
                            Value::Bool(e.required),
                            binary(&e.data),
                        ])
                    })
                    .collect(),
            ),
        ]))
    }
    fn validate(&self) -> Result<()> {
        if self.major != 1 {
            return Err(Error::UnsupportedMajor(self.major));
        }
        if !token(&self.kind, 64)
            || self.schema == 0
            || !token(&self.scope.resource, 256)
            || !token(&self.scope.audience, 256)
            || self.actor.incarnation == [0; 16]
        {
            return Err(Error::InvalidField);
        }
        if self.body_length as usize > MAX_BODY {
            return Err(Error::BodyTooLarge);
        }
        if self.sequence == 0 || (self.sequence == 1) != self.previous.is_none() {
            return Err(Error::InvalidSequence);
        }
        if self.dependencies.len() > MAX_DEPENDENCIES
            || self.dependencies.windows(2).any(|w| w[0] >= w[1])
            || self.extensions.len() > MAX_EXTENSIONS
            || self.extensions.windows(2).any(|w| w[0].id >= w[1].id)
            || self
                .extensions
                .iter()
                .any(|e| e.id == 0 || e.data.len() > 1024)
        {
            return Err(Error::InvalidField);
        }
        if self.stream != stream_id(&self.actor, &self.scope)? {
            return Err(Error::WrongStream);
        }
        Ok(())
    }
}
fn uint(value: &Value) -> Result<u64> {
    value
        .as_integer()
        .and_then(|n| u64::try_from(n).ok())
        .ok_or(Error::Malformed)
}
fn bytes<const N: usize>(value: &Value) -> Result<[u8; N]> {
    value
        .as_bytes()
        .ok_or(Error::Malformed)?
        .as_slice()
        .try_into()
        .map_err(|_| Error::Malformed)
}
fn text(value: &Value) -> Result<String> {
    Ok(value.as_text().ok_or(Error::Malformed)?.to_owned())
}
fn small(value: &Value) -> Result<u16> {
    uint(value)?.try_into().map_err(|_| Error::Malformed)
}
fn maybe_id(value: &Value) -> Result<Option<EventId>> {
    if value == &Value::Null {
        Ok(None)
    } else {
        Ok(Some(EventId(bytes(value)?)))
    }
}
fn parse(payload: &[u8]) -> Result<Metadata> {
    let value: Value =
        ciborium::de::from_reader_with_recursion_limit(payload, 8).map_err(|_| Error::Malformed)?;
    let fields = value.as_array().ok_or(Error::Malformed)?;
    let domain = fields
        .first()
        .and_then(Value::as_text)
        .ok_or(Error::Malformed)?;
    if domain != DOMAIN {
        return Err(if domain.starts_with("bbsbox.event/") {
            Error::UnsupportedDomain
        } else {
            Error::Malformed
        });
    }
    let major = small(fields.get(1).ok_or(Error::Malformed)?)?;
    if major != 1 {
        return Err(Error::UnsupportedMajor(major));
    }
    let minor = small(fields.get(2).ok_or(Error::Malformed)?)?;
    if fields.len() != 17 && minor != 0 {
        return Err(Error::UnsupportedMinor(minor));
    }
    if fields.len() > 17 {
        return Err(Error::UnsupportedStructure);
    }
    if fields.len() != 17 {
        return Err(Error::Malformed);
    }
    let actor = fields[5].as_array().ok_or(Error::Malformed)?;
    if actor.len() != 5 {
        return Err(Error::Malformed);
    }
    let role = Role::parse(uint(&actor[0])?.try_into().map_err(|_| Error::Malformed)?)?;
    let credential = if actor[3] == Value::Null {
        None
    } else {
        Some(CredentialId::from_bytes(bytes(&actor[3])?))
    };
    let distribution = match uint(&fields[11])? {
        0 => Distribution::Local,
        1 => Distribution::Shared,
        _ => return Err(Error::InvalidField),
    };
    let dependencies = fields[12]
        .as_array()
        .ok_or(Error::Malformed)?
        .iter()
        .map(|v| {
            let fields = v.as_array().ok_or(Error::Malformed)?;
            if fields.len() != 2 {
                return Err(Error::Malformed);
            }
            let kind = match uint(&fields[0])?.try_into().map_err(|_| Error::Malformed)? {
                1 => DependencyKind::Event,
                2 => DependencyKind::Credential,
                3 => DependencyKind::Blob,
                other => return Err(Error::UnsupportedDependency(other)),
            };
            Ok(Dependency {
                kind,
                digest: bytes(&fields[1])?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let extensions = fields[16]
        .as_array()
        .ok_or(Error::Malformed)?
        .iter()
        .map(|value| {
            let fields = value.as_array().ok_or(Error::Malformed)?;
            if fields.len() != 3 {
                return Err(Error::Malformed);
            }
            Ok(Extension {
                id: small(&fields[0])?,
                required: fields[1].as_bool().ok_or(Error::Malformed)?,
                data: fields[2].as_bytes().ok_or(Error::Malformed)?.to_vec(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let metadata = Metadata {
        major: small(&fields[1])?,
        minor: small(&fields[2])?,
        kind: text(&fields[3])?,
        schema: small(&fields[4])?,
        actor: Actor {
            role,
            subject: bytes(&actor[1])?,
            key: bytes(&actor[2])?,
            credential,
            incarnation: bytes(&actor[4])?,
        },
        stream: bytes(&fields[6])?,
        sequence: uint(&fields[7])?,
        previous: maybe_id(&fields[8])?,
        scope: Scope {
            resource: text(&fields[9])?,
            audience: text(&fields[10])?,
            distribution,
        },
        dependencies,
        claimed_time: fields[13]
            .as_integer()
            .and_then(|n| i64::try_from(n).ok())
            .ok_or(Error::Malformed)?,
        body_hash: bytes(&fields[14])?,
        body_length: uint(&fields[15])?
            .try_into()
            .map_err(|_| Error::BodyTooLarge)?,
        extensions,
    };
    metadata.validate()?;
    if metadata.payload()? != payload {
        return Err(Error::NonCanonical);
    }
    Ok(metadata)
}
fn unsigned(metadata: &Metadata) -> Result<CoseSign1> {
    Ok(CoseSign1Builder::new()
        .protected(
            HeaderBuilder::new()
                .algorithm(iana::Algorithm::Ed25519)
                .key_id(metadata.actor.key.to_vec())
                .build(),
        )
        .payload(metadata.payload()?)
        .build())
}
#[derive(Clone, Debug)]
pub struct Commitment {
    id: EventId,
    encoded: Vec<u8>,
    metadata: Metadata,
    credential: Option<Credential>,
}
impl Commitment {
    pub fn id(&self) -> EventId {
        self.id
    }
    pub fn encoded(&self) -> &[u8] {
        &self.encoded
    }
    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }
    pub fn credential(&self) -> Option<&Credential> {
        self.credential.as_ref()
    }
    pub fn with_body(self, body: &[u8]) -> Result<CompleteEvent> {
        if body.len() > MAX_BODY {
            return Err(Error::BodyTooLarge);
        }
        if body.len() != self.metadata.body_length as usize
            || Sha256::digest(body).as_slice() != self.metadata.body_hash
        {
            return Err(Error::BodyMismatch);
        }
        Ok(CompleteEvent {
            commitment: self,
            body: body.to_vec(),
        })
    }
}
#[derive(Clone)]
pub struct CompleteEvent {
    commitment: Commitment,
    body: Vec<u8>,
}
impl fmt::Debug for CompleteEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CompleteEvent")
            .field("id", &self.id())
            .field("body_bytes", &self.body.len())
            .finish_non_exhaustive()
    }
}
impl CompleteEvent {
    pub fn id(&self) -> EventId {
        self.commitment.id
    }
    pub fn metadata(&self) -> &Metadata {
        &self.commitment.metadata
    }
    pub fn commitment(&self) -> &Commitment {
        &self.commitment
    }
    pub fn body(&self) -> &[u8] {
        &self.body
    }
}
pub fn verify(
    encoded: &[u8],
    resolve: impl FnOnce(CredentialId) -> Option<Credential>,
) -> Result<Commitment> {
    verify_with_resolver(encoded, |id| Ok(resolve(id)))
}
pub fn verify_with_resolver(
    encoded: &[u8],
    resolve: impl FnOnce(CredentialId) -> Result<Option<Credential>>,
) -> Result<Commitment> {
    verify_signature(encoded)?.bind(resolve)
}
/// Signature-only staging value. Actor claims are not yet a verified person/delegation or action grant.
#[derive(Clone, Debug)]
pub struct SignedEnvelope {
    id: EventId,
    encoded: Vec<u8>,
    metadata: Metadata,
}
impl SignedEnvelope {
    pub fn id(&self) -> EventId {
        self.id
    }
    pub fn encoded(&self) -> &[u8] {
        &self.encoded
    }
    pub fn claims(&self) -> &Metadata {
        &self.metadata
    }
    pub fn check_body(&self, body: &[u8]) -> Result<()> {
        if body.len() > MAX_BODY {
            return Err(Error::BodyTooLarge);
        }
        if body.len() != self.metadata.body_length as usize
            || Sha256::digest(body).as_slice() != self.metadata.body_hash
        {
            return Err(Error::BodyMismatch);
        }
        Ok(())
    }
    pub fn bind(
        self,
        resolve: impl FnOnce(CredentialId) -> Result<Option<Credential>>,
    ) -> Result<Commitment> {
        bind_signature(self, resolve)
    }
}
pub fn verify_signature(encoded: &[u8]) -> Result<SignedEnvelope> {
    if encoded.len() > MAX_ENVELOPE {
        return Err(Error::EnvelopeTooLarge);
    }
    let cose = CoseSign1::from_slice(encoded).map_err(|_| Error::Malformed)?;
    let metadata = parse(cose.payload.as_ref().ok_or(Error::Malformed)?)?;
    let mut canonical = unsigned(&metadata)?;
    canonical.signature = cose.signature.clone();
    if canonical.to_vec().map_err(|_| Error::Malformed)? != encoded {
        return Err(Error::NonCanonical);
    }
    let key = VerifyingKey::from_bytes(&metadata.actor.key).map_err(|_| Error::InvalidKey)?;
    if key.is_weak() {
        return Err(Error::InvalidKey);
    }
    key.verify_strict(
        &cose.tbs_data(DOMAIN.as_bytes()),
        &Signature::try_from(cose.signature.as_slice()).map_err(|_| Error::InvalidSignature)?,
    )
    .map_err(|_| Error::InvalidSignature)?;
    Ok(SignedEnvelope {
        id: EventId(Sha256::digest(encoded).into()),
        encoded: encoded.to_vec(),
        metadata,
    })
}
fn bind_signature(
    signed: SignedEnvelope,
    resolve: impl FnOnce(CredentialId) -> Result<Option<Credential>>,
) -> Result<Commitment> {
    let metadata = signed.metadata;
    let actor = &metadata.actor;
    let credential = if actor.role == Role::PersonDevice {
        let id = actor.credential.ok_or(Error::CredentialMismatch)?;
        let credential = resolve(id)?.ok_or(Error::MissingCredential(id))?;
        if credential.id() != id
            || credential.body().device() != &actor.key
            || credential.body().person_id().bytes() != &actor.subject
            || credential.body().incarnation() != &actor.incarnation
        {
            return Err(Error::CredentialMismatch);
        }
        Some(credential)
    } else {
        if actor.credential.is_some()
            || Actor::root(actor.role, actor.key, actor.incarnation)?.subject != actor.subject
        {
            return Err(Error::CredentialMismatch);
        }
        None
    };
    Ok(Commitment {
        id: signed.id,
        encoded: signed.encoded,
        metadata,
        credential,
    })
}
/// Local creation fixes protocol 1.0 and derives actor/stream/body commitments before asking a key holder to sign.
pub fn sign(
    draft: Draft,
    actor: Actor,
    body: &[u8],
    credential: Option<&Credential>,
    signer: impl FnOnce(&[u8]) -> Result<[u8; 64]>,
) -> Result<CompleteEvent> {
    if body.len() > MAX_BODY {
        return Err(Error::BodyTooLarge);
    }
    let metadata = Metadata {
        major: 1,
        minor: 0,
        kind: draft.kind,
        schema: draft.schema,
        stream: stream_id(&actor, &draft.scope)?,
        actor,
        sequence: draft.sequence,
        previous: draft.previous,
        scope: draft.scope,
        dependencies: draft.dependencies,
        claimed_time: draft.claimed_time,
        body_hash: Sha256::digest(body).into(),
        body_length: body.len() as u32,
        extensions: draft.extensions,
    };
    metadata.validate()?;
    let unsigned = unsigned(&metadata)?;
    let mut estimate = unsigned.clone();
    estimate.signature = vec![0; 64];
    if estimate.to_vec().map_err(|_| Error::Malformed)?.len() > MAX_ENVELOPE {
        return Err(Error::EnvelopeTooLarge);
    }
    let signature = signer(&unsigned.tbs_data(DOMAIN.as_bytes()))?;
    let encoded = CoseSign1Builder::new()
        .protected(unsigned.protected.header)
        .payload(unsigned.payload.ok_or(Error::Malformed)?)
        .signature(signature.to_vec())
        .build()
        .to_vec()
        .map_err(|_| Error::Malformed)?;
    verify(&encoded, |id| credential.filter(|c| c.id() == id).cloned())?.with_body(body)
}
