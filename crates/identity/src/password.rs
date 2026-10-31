// SPDX-License-Identifier: AGPL-3.0-or-later
//! CPU/memory work belongs on a bounded authentication worker, never the SQLite writer/async executor.
use crate::{Error, Result};
use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};
use std::sync::atomic::{AtomicUsize, Ordering};
use zeroize::Zeroizing;

pub struct Engine {
    in_flight: AtomicUsize,
    limit: usize,
}
pub struct PreparedPassword(Zeroizing<String>);
impl PreparedPassword {
    pub(crate) fn encoded(&self) -> &str {
        &self.0
    }
}
struct Permit<'a>(&'a AtomicUsize);
impl Drop for Permit<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
impl Default for Engine {
    fn default() -> Self {
        Self {
            in_flight: AtomicUsize::new(0),
            limit: 2,
        }
    }
}
impl Engine {
    pub fn new(limit: usize) -> Result<Self> {
        if limit == 0 || limit > 16 {
            return Err(Error::InvalidConfiguration);
        }
        Ok(Self {
            in_flight: AtomicUsize::new(0),
            limit,
        })
    }
    fn permit(&self) -> Result<Permit<'_>> {
        self.in_flight
            .try_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < self.limit).then_some(n + 1)
            })
            .map_err(|_| Error::Busy)?;
        Ok(Permit(&self.in_flight))
    }
    fn algorithm() -> Argon2<'static> {
        Argon2::new(
            Algorithm::Argon2id,
            Version::V0x13,
            Params::new(19456, 2, 1, Some(32)).expect("fixed valid Argon2 parameters"),
        )
    }
    pub fn prepare(&self, password: Zeroizing<String>) -> Result<PreparedPassword> {
        if password.chars().count() < 15 || password.len() > 1024 {
            return Err(Error::WeakPassword);
        }
        let _permit = self.permit()?;
        let encoded = Self::algorithm()
            .hash_password(password.as_bytes())
            .map_err(|_| Error::InvalidStoredHash)?
            .to_string();
        Ok(PreparedPassword(Zeroizing::new(encoded)))
    }
    pub(crate) fn verify(&self, password: &str, encoded: &str) -> Result<bool> {
        if password.len() > 1024 {
            return Err(Error::InvalidCredentials);
        }
        let _permit = self.permit()?;
        let hash = PasswordHash::new(encoded).map_err(|_| Error::InvalidStoredHash)?;
        // Never let a damaged or unrecognized database record select unbounded hashing parameters.
        if hash.algorithm.as_str() != "argon2id"
            || hash.version != Some(19)
            || hash.params.get("m").and_then(|p| p.decimal().ok()) != Some(19456)
            || hash.params.get("t").and_then(|p| p.decimal().ok()) != Some(2)
            || hash.params.get("p").and_then(|p| p.decimal().ok()) != Some(1)
            || hash.hash.as_ref().is_none_or(|h| h.as_bytes().len() != 32)
        {
            return Err(Error::InvalidStoredHash);
        }
        match Self::algorithm().verify_password(password.as_bytes(), &hash) {
            Ok(()) => Ok(true),
            Err(argon2::password_hash::Error::PasswordInvalid) => Ok(false),
            Err(_) => Err(Error::InvalidStoredHash),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn busy_password_worker_rejects_without_waiting_and_recovers_after_release() {
        let engine = Engine::new(1).unwrap();
        let permit = engine.permit().unwrap();
        assert!(matches!(
            engine.prepare(Zeroizing::new("a full test passphrase".into())),
            Err(Error::Busy)
        ));
        drop(permit);
        assert!(
            engine
                .prepare(Zeroizing::new("a full test passphrase".into()))
                .is_ok()
        );
    }
}
