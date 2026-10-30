// SPDX-License-Identifier: AGPL-3.0-or-later
//! Disposable M00 wire experiment. This is not the BBSBOX v1 protocol.
use ciborium::value::Value;
use coset::{CborSerializable, CoseSign1, CoseSign1Builder, HeaderBuilder, iana};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};

pub type LabResult<T> = Result<T, String>;
pub const DOMAIN: &str = "bbsbox.m00.event/v0";
pub const RESOURCE: &str = "m00.messagebook";

pub fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
pub fn encode(value: &Value) -> LabResult<Vec<u8>> {
    let mut bytes = Vec::new();
    ciborium::ser::into_writer(value, &mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}
fn number(n: u64) -> Value {
    Value::Integer(n.into())
}

fn commitment(
    public: &[u8; 32],
    incarnation: &str,
    sequence: u64,
    previous: Option<&str>,
    label: &str,
    text: &str,
    time: u64,
) -> LabResult<(Vec<u8>, Vec<u8>)> {
    let body = encode(&Value::Array(vec![
        Value::Text(label.into()),
        Value::Text(text.into()),
    ]))?;
    let stream = format!(
        "{}:{RESOURCE}:local-public:{incarnation}",
        hex::encode(public)
    );
    let payload = encode(&Value::Array(vec![
        Value::Text(DOMAIN.into()),
        number(0),
        number(0),
        Value::Text("test.message".into()),
        number(1),
        Value::Bytes(public.to_vec()),
        Value::Text(stream),
        number(sequence),
        previous
            .map(|p| hex::decode(p).map(Value::Bytes))
            .transpose()
            .map_err(|e| e.to_string())?
            .unwrap_or(Value::Null),
        Value::Text(RESOURCE.into()),
        Value::Array(vec![]),
        Value::Text("local-public".into()),
        number(time),
        Value::Bytes(Sha256::digest(&body).to_vec()),
        number(body.len() as u64),
    ]))?;
    Ok((payload, body))
}
pub fn sign(
    key: &SigningKey,
    incarnation: &str,
    sequence: u64,
    previous: Option<&str>,
    label: &str,
    text: &str,
    time: u64,
) -> LabResult<(String, Vec<u8>, Vec<u8>)> {
    let public = key.verifying_key().to_bytes();
    let (payload, body) = commitment(&public, incarnation, sequence, previous, label, text, time)?;
    let envelope = CoseSign1Builder::new()
        .protected(
            HeaderBuilder::new()
                .algorithm(iana::Algorithm::EdDSA)
                .key_id(public.to_vec())
                .build(),
        )
        .payload(payload)
        .create_signature(DOMAIN.as_bytes(), |bytes| {
            key.sign(bytes).to_bytes().to_vec()
        })
        .build()
        .to_vec()
        .map_err(|e| e.to_string())?;
    Ok((digest(&envelope), envelope, body))
}

pub struct Verified {
    pub label: String,
    pub text: String,
    pub time: u64,
}
pub fn verify(
    key: &VerifyingKey,
    incarnation: &str,
    sequence: u64,
    previous: Option<&str>,
    id: &str,
    envelope: &[u8],
    body: &[u8],
) -> LabResult<Verified> {
    if digest(envelope) != id {
        return Err("event ID mismatch".into());
    }
    let cose = CoseSign1::from_slice(envelope).map_err(|e| e.to_string())?;
    let expected_header = HeaderBuilder::new()
        .algorithm(iana::Algorithm::EdDSA)
        .key_id(key.to_bytes().to_vec())
        .build();
    if cose.protected.header != expected_header || cose.unprotected != Default::default() {
        return Err("unsupported signing headers".into());
    }
    cose.verify_signature(DOMAIN.as_bytes(), |signature, bytes| {
        let signature = Signature::from_slice(signature).map_err(|_| "invalid signature length")?;
        key.verify_strict(bytes, &signature)
            .map_err(|_| "invalid signature")
    })
    .map_err(str::to_string)?;
    let body_value: Value = ciborium::de::from_reader(body).map_err(|e| e.to_string())?;
    let fields = body_value.as_array().ok_or("invalid message body")?;
    if fields.len() != 2 {
        return Err("invalid message fields".into());
    }
    let label = fields[0].as_text().ok_or("invalid label")?.to_string();
    let text = fields[1].as_text().ok_or("invalid message")?.to_string();
    let payload = cose.payload.as_ref().ok_or("missing commitment")?;
    let value: Value = ciborium::de::from_reader(payload.as_slice()).map_err(|e| e.to_string())?;
    let fields = value.as_array().ok_or("invalid envelope")?;
    let time: u64 = fields
        .get(12)
        .and_then(Value::as_integer)
        .ok_or("missing time")?
        .try_into()
        .map_err(|_| "invalid time")?;
    // Recreate our narrowly defined deterministic profile. Also rejects extra/trailing fields/bytes.
    let (expected_payload, expected_body) = commitment(
        &key.to_bytes(),
        incarnation,
        sequence,
        previous,
        &label,
        &text,
        time,
    )?;
    if *payload != expected_payload || expected_body != body {
        return Err("body, scope or chain mismatch".into());
    }
    // coset preserves signed protected bytes; only our deterministic representation is accepted.
    let mut canonical = cose.clone();
    canonical.protected.original_data = None;
    if canonical.to_vec().map_err(|e| e.to_string())? != envelope {
        return Err("noncanonical envelope".into());
    }
    Ok(Verified { label, text, time })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signed_round_trip_rejects_tampering_wrong_keys_and_chains() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let (id, envelope, body) = sign(
            &key,
            "writer-a",
            1,
            None,
            "Neighbor",
            "Hello, neighborhood!",
            42,
        )
        .unwrap();
        assert_eq!(
            verify(
                &key.verifying_key(),
                "writer-a",
                1,
                None,
                &id,
                &envelope,
                &body
            )
            .unwrap()
            .text,
            "Hello, neighborhood!"
        );
        let mut corrupt = body.clone();
        corrupt[1] ^= 1;
        assert!(
            verify(
                &key.verifying_key(),
                "writer-a",
                1,
                None,
                &id,
                &envelope,
                &corrupt
            )
            .is_err()
        );
        let mut corrupt = envelope.clone();
        let end = corrupt.len() - 1;
        corrupt[end] ^= 1;
        assert!(
            verify(
                &key.verifying_key(),
                "writer-a",
                1,
                None,
                &digest(&corrupt),
                &corrupt,
                &body
            )
            .is_err()
        );
        assert!(
            verify(
                &SigningKey::from_bytes(&[8; 32]).verifying_key(),
                "writer-a",
                1,
                None,
                &id,
                &envelope,
                &body
            )
            .is_err()
        );
        assert!(
            verify(
                &key.verifying_key(),
                "writer-a",
                2,
                None,
                &id,
                &envelope,
                &body
            )
            .is_err()
        );
        assert!(
            verify(
                &key.verifying_key(),
                "writer-b",
                1,
                None,
                &id,
                &envelope,
                &body
            )
            .is_err()
        );
        let (_, same, _) = sign(
            &key,
            "writer-a",
            1,
            None,
            "Neighbor",
            "Hello, neighborhood!",
            42,
        )
        .unwrap();
        assert_eq!(envelope, same);
    }
}
