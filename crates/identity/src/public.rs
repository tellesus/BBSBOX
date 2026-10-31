// SPDX-License-Identifier: AGPL-3.0-or-later
//! Public resolver only: storing a verified delegation does not link a local account or grant admission.
use crate::{Error, Result};
use bbsbox_credentials::{Credential, CredentialId};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

pub fn store_credential(tx: &Transaction<'_>, credential: &Credential) -> Result<bool> {
    Ok(tx.execute(
        "INSERT OR IGNORE INTO identity_device_credentials VALUES(?1,?2,?3)",
        params![
            credential.id().bytes().as_slice(),
            credential.body().person_id().bytes().as_slice(),
            credential.encoded()
        ],
    )? == 1)
}
/// Missing credentials remain missing dependencies. Known revocation/resource policy belongs to M04's authorizer.
pub fn resolve_credential(reader: &Connection, id: CredentialId) -> Result<Option<Credential>> {
    let row: Option<(Vec<u8>, Vec<u8>)> = reader
        .query_row(
            "SELECT encoded,person_id FROM identity_device_credentials WHERE id=?1",
            [id.bytes().as_slice()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((encoded, person)) = row else {
        return Ok(None);
    };
    let credential = Credential::verify(&encoded)?;
    if credential.id() != id || credential.body().person_id().bytes().as_slice() != person {
        return Err(Error::InvalidStoredCredential);
    }
    Ok(Some(credential))
}
