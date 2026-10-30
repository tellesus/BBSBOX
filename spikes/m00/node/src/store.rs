// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::event::{self, LabResult};
use ed25519_dalek::SigningKey;
use rand_core::{OsRng, RngCore};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Serialize, Clone, Debug)]
pub struct Message {
    pub id: String,
    pub sequence: u64,
    pub label: String,
    pub text: String,
    pub time: u64,
    pub verified: bool,
    pub envelope_hex: String,
    pub body_hex: String,
}
pub struct Store {
    db: Connection,
    key: SigningKey,
    incarnation: String,
}
impl Store {
    pub fn open(path: &Path, key: SigningKey) -> LabResult<Self> {
        let db = Connection::open(path).map_err(|e| e.to_string())?;
        db.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS metadata (name TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS messages (sequence INTEGER PRIMARY KEY, id TEXT NOT NULL UNIQUE,
              request_id TEXT NOT NULL UNIQUE, envelope BLOB NOT NULL, body BLOB NOT NULL);")
            .map_err(|e| e.to_string())?;
        let public = hex::encode(key.verifying_key().to_bytes());
        let old: Option<String> = db
            .query_row("SELECT value FROM metadata WHERE name='signer'", [], |r| {
                r.get(0)
            })
            .optional()
            .map_err(|e| e.to_string())?;
        if old.as_ref().is_some_and(|old| *old != public) {
            return Err(
                "database signer differs from fixture key; restore the matching key".into(),
            );
        }
        db.execute(
            "INSERT OR IGNORE INTO metadata VALUES ('signer', ?1)",
            [&public],
        )
        .map_err(|e| e.to_string())?;
        let mut random = [0u8; 16];
        OsRng.fill_bytes(&mut random);
        db.execute(
            "INSERT OR IGNORE INTO metadata VALUES ('incarnation', ?1)",
            [hex::encode(random)],
        )
        .map_err(|e| e.to_string())?;
        let incarnation = db
            .query_row(
                "SELECT value FROM metadata WHERE name='incarnation'",
                [],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let store = Self {
            db,
            key,
            incarnation,
        };
        store.messages()?; // Refuse to start over a corrupted chain.
        Ok(store)
    }
    pub fn public_key(&self) -> String {
        hex::encode(self.key.verifying_key().to_bytes())
    }
    pub fn sqlite_version(&self) -> &'static str {
        rusqlite::version()
    }
    pub fn messages(&self) -> LabResult<Vec<Message>> {
        let mut query = self
            .db
            .prepare("SELECT sequence,id,envelope,body FROM messages ORDER BY sequence")
            .map_err(|e| e.to_string())?;
        let rows = query
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Vec<u8>>(2)?,
                    r.get::<_, Vec<u8>>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut previous = None;
        let mut messages = Vec::new();
        for row in rows {
            let (sequence, id, envelope, body) = row.map_err(|e| e.to_string())?;
            let sequence = u64::try_from(sequence).map_err(|_| "negative sequence")?;
            if sequence != messages.len() as u64 + 1 {
                return Err("message chain has a gap".into());
            }
            let checked = event::verify(
                &self.key.verifying_key(),
                &self.incarnation,
                sequence,
                previous.as_deref(),
                &id,
                &envelope,
                &body,
            )?;
            previous = Some(id.clone());
            messages.push(Message {
                id,
                sequence,
                label: checked.label,
                text: checked.text,
                time: checked.time,
                verified: true,
                envelope_hex: hex::encode(envelope),
                body_hex: hex::encode(body),
            });
        }
        Ok(messages)
    }
    pub fn append(&mut self, request: &str, label: &str, text: &str) -> LabResult<Message> {
        let existing: Option<String> = self
            .db
            .query_row(
                "SELECT id FROM messages WHERE request_id=?1",
                [request],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let messages = self.messages()?;
        if let Some(id) = existing {
            let message = messages
                .into_iter()
                .find(|m| m.id == id)
                .ok_or("missing retry result")?;
            if message.label != label || message.text != text {
                return Err("retry ID already belongs to a different message".into());
            }
            return Ok(message);
        }
        if messages.len() >= 1000 {
            return Err("the 1,000-message experiment limit has been reached".into());
        }
        let sequence = messages.len() as u64 + 1;
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs();
        let (id, envelope, body) = event::sign(
            &self.key,
            &self.incarnation,
            sequence,
            messages.last().map(|m| m.id.as_str()),
            label,
            text,
            time,
        )?;
        let tx = self.db.transaction().map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO messages VALUES (?1,?2,?3,?4,?5)",
            params![
                i64::try_from(sequence).map_err(|_| "sequence overflow")?,
                id,
                request,
                envelope,
                body
            ],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(Message {
            id,
            sequence,
            label: label.into(),
            text: text.into(),
            time,
            verified: true,
            envelope_hex: hex::encode(envelope),
            body_hex: hex::encode(body),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restart_and_retries_preserve_messages_and_signer() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.sqlite");
        let key = SigningKey::from_bytes(&[3; 32]);
        let mut store = Store::open(&path, key.clone()).unwrap();
        let first = store
            .append("retry-1", "Neighbor", "A local hello")
            .unwrap();
        assert_eq!(
            store
                .append("retry-1", "Neighbor", "A local hello")
                .unwrap()
                .id,
            first.id
        );
        assert!(store.append("retry-1", "Neighbor", "Different").is_err());
        drop(store);
        let mut store = Store::open(&path, key.clone()).unwrap();
        assert_eq!(store.messages().unwrap().len(), 1);
        assert_eq!(
            store
                .append("retry-2", "Next", "Still here")
                .unwrap()
                .sequence,
            2
        );
        drop(store);
        assert!(Store::open(&path, SigningKey::from_bytes(&[4; 32])).is_err());
        let db = Connection::open(&path).unwrap();
        db.execute("UPDATE messages SET body=x'00' WHERE sequence=1", [])
            .unwrap();
        drop(db);
        assert!(Store::open(&path, key).is_err());
    }
}
