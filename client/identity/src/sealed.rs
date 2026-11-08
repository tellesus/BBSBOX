// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared fixed-profile encrypted client records. Only ciphertext leaves this module.
use crate::recovery::{Engine, Error, Permit, derive, passphrase};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use ciborium::value::Value;
use rand_core::{OsRng, RngCore};
use zeroize::Zeroizing;

pub(crate) struct Sealer {
    domain: &'static str,
    maximum: usize,
    salt: [u8; 16],
    key: Zeroizing<[u8; 32]>,
}

fn encode(value: &Value) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    ciborium::ser::into_writer(value, &mut bytes).map_err(|_| Error::InvalidKit)?;
    Ok(bytes)
}

fn header(domain: &str, salt: [u8; 16], nonce: [u8; 24]) -> Vec<Value> {
    vec![
        Value::Text(domain.into()),
        Value::Integer(1.into()),
        Value::Integer(1.into()),
        Value::Bytes(salt.to_vec()),
        Value::Bytes(nonce.to_vec()),
    ]
}

impl Sealer {
    // Callers retain the permit until their complete plaintext/key operation has finished.
    pub(crate) fn create<'a>(
        engine: &'a Engine,
        domain: &'static str,
        maximum: usize,
        pass: Zeroizing<String>,
    ) -> Result<(Self, Permit<'a>), Error> {
        passphrase(&pass)?;
        let permit = engine.permit()?;
        let mut salt = [0; 16];
        OsRng.fill_bytes(&mut salt);
        Ok((
            Self {
                domain,
                maximum,
                salt,
                key: derive(&pass, &salt)?,
            },
            permit,
        ))
    }

    pub(crate) fn open<'a>(
        engine: &'a Engine,
        domain: &'static str,
        minimum: usize,
        maximum: usize,
        encoded: &[u8],
        pass: Zeroizing<String>,
    ) -> Result<(Self, Zeroizing<Vec<u8>>, Permit<'a>), Error> {
        passphrase(&pass)?;
        if encoded.len() > maximum + 256 {
            return Err(Error::InvalidKit);
        }
        let value: Value = ciborium::de::from_reader_with_recursion_limit(encoded, 4)
            .map_err(|_| Error::InvalidKit)?;
        let fields = value.as_array().ok_or(Error::InvalidKit)?;
        if fields.len() != 6 || fields[0].as_text() != Some(domain) {
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
        if !(minimum + 16..=maximum + 16).contains(&ciphertext.len()) || encode(&value)? != encoded
        {
            return Err(Error::InvalidKit);
        }
        let permit = engine.permit()?;
        let key = derive(&pass, &salt)?;
        let cipher =
            XChaCha20Poly1305::new_from_slice(key.as_slice()).map_err(|_| Error::InvalidKit)?;
        let aad = encode(&Value::Array(header(domain, salt, nonce)))?;
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
        Ok((
            Self {
                domain,
                maximum,
                salt,
                key,
            },
            plaintext,
            permit,
        ))
    }

    pub(crate) fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>, Error> {
        if plaintext.len() > self.maximum {
            return Err(Error::InvalidKit);
        }
        let mut nonce = [0; 24];
        OsRng.fill_bytes(&mut nonce);
        let mut fields = header(self.domain, self.salt, nonce);
        let aad = encode(&Value::Array(fields.clone()))?;
        let cipher = XChaCha20Poly1305::new_from_slice(self.key.as_slice())
            .map_err(|_| Error::InvalidKit)?;
        fields.push(Value::Bytes(
            cipher
                .encrypt(
                    &XNonce::from(nonce),
                    Payload {
                        msg: plaintext,
                        aad: &aad,
                    },
                )
                .map_err(|_| Error::InvalidKit)?,
        ));
        encode(&Value::Array(fields))
    }
}
