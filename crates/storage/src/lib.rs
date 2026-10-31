// SPDX-License-Identifier: AGPL-3.0-or-later
//! M02 A: owned SQLite writer, ordered migrations and atomic idempotent commands.
//! The bounded writer actor, durable jobs, blobs and snapshot coordinator follow in B/C.
use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fmt, path::Path, time::Duration};

const APPLICATION_ID: i32 = 0x42425358;
const SCHEMA_VERSION: i32 = 1;
const MAX_RECEIPT_BYTES: usize = 1024 * 1024;
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Sqlite(rusqlite::Error),
    ForeignDatabase,
    UnsupportedSchema(i32),
    InvalidMigration,
    ChangedOrUnknownMigration,
    InvalidCommandKey,
    ReusedCommandId,
    ReceiptTooLarge,
    WalUnavailable,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite(error) => write!(f, "SQLite operation failed: {error}"),
            Self::ForeignDatabase => f.write_str("this is not a BBSBOX storage database"),
            Self::UnsupportedSchema(version) => {
                write!(f, "unsupported core schema version {version}")
            }
            Self::InvalidMigration => {
                f.write_str("migration IDs must be unique and contiguous from version 1")
            }
            Self::ChangedOrUnknownMigration => {
                f.write_str("an applied migration was changed, removed or is newer than this code")
            }
            Self::InvalidCommandKey => {
                f.write_str("command namespace and ID must be 1–128 ASCII identifier characters")
            }
            Self::ReusedCommandId => {
                f.write_str("the command ID was already used with different request bytes")
            }
            Self::ReceiptTooLarge => f.write_str("the command response exceeds the receipt limit"),
            Self::WalUnavailable => f.write_str("SQLite WAL is unavailable on this storage path"),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sqlite(error) => Some(error),
            _ => None,
        }
    }
}
impl From<rusqlite::Error> for Error {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sqlite(value)
    }
}

/// All compiled module migrations remain registered even when a module is disabled.
pub struct Migration {
    pub module: &'static str,
    pub version: u32,
    pub sql: &'static str,
}
pub struct Command<'a> {
    pub namespace: &'a str,
    pub id: &'a str,
    pub request: &'a [u8],
}
#[derive(Debug, PartialEq, Eq)]
pub struct Receipt {
    pub bytes: Vec<u8>,
    pub replayed: bool,
}

/// This connection has one owner and is deliberately not shared across async request threads.
/// M02 B will place it behind the bounded writer service. Until then this is a tested library,
/// not a production HTTP integration.
pub struct Database {
    connection: Connection,
}

fn valid_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-.:/".contains(&b))
}
fn configure(connection: &Connection, writable: bool) -> Result<()> {
    connection.busy_timeout(Duration::from_secs(5))?;
    connection.pragma_update(None, "foreign_keys", true)?;
    if writable {
        let mode: String = connection.pragma_query_value(None, "journal_mode", |r| r.get(0))?;
        if !mode.eq_ignore_ascii_case("wal") {
            connection.pragma_update(None, "journal_mode", "WAL")?;
        }
        let mode: String = connection.pragma_query_value(None, "journal_mode", |r| r.get(0))?;
        if !mode.eq_ignore_ascii_case("wal") {
            return Err(Error::WalUnavailable);
        }
        connection.pragma_update(None, "synchronous", "FULL")?;
    } else {
        connection.pragma_update(None, "query_only", true)?;
    }
    Ok(())
}
impl Database {
    pub fn open(path: impl AsRef<Path>, migrations: &[Migration]) -> Result<Self> {
        let mut known = BTreeMap::new();
        for migration in migrations {
            if !valid_key(migration.module)
                || migration.version == 0
                || known
                    .insert((migration.module, migration.version), migration)
                    .is_some()
            {
                return Err(Error::InvalidMigration);
            }
        }
        let mut previous_module = "";
        let mut previous_version = 0;
        for &(module, version) in known.keys() {
            if module != previous_module {
                previous_module = module;
                previous_version = 0;
            }
            if version != previous_version + 1 {
                return Err(Error::InvalidMigration);
            }
            previous_version = version;
        }
        let mut connection = Connection::open(path)?;
        let application: i32 =
            connection.pragma_query_value(None, "application_id", |r| r.get(0))?;
        let version: i32 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
        let tables: i64 = connection.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%'",
            [],
            |r| r.get(0),
        )?;
        let fresh = application == 0 && version == 0 && tables == 0;
        if !fresh && application != APPLICATION_ID {
            return Err(Error::ForeignDatabase);
        }
        if !fresh && version != SCHEMA_VERSION {
            return Err(Error::UnsupportedSchema(version));
        }
        configure(&connection, true)?;
        let tx = connection.transaction()?;
        if fresh {
            tx.execute_batch("CREATE TABLE core_migrations(module TEXT NOT NULL,version INTEGER NOT NULL,digest BLOB NOT NULL,PRIMARY KEY(module,version));
                CREATE TABLE core_command_receipts(namespace TEXT NOT NULL,command_id TEXT NOT NULL,request_hash BLOB NOT NULL,response BLOB NOT NULL,PRIMARY KEY(namespace,command_id));")?;
            tx.pragma_update(None, "application_id", APPLICATION_ID)?;
            tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        let applied = {
            let mut query = tx.prepare(
                "SELECT module,version,digest FROM core_migrations ORDER BY module,version",
            )?;
            query
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, u32>(1)?,
                        r.get::<_, Vec<u8>>(2)?,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };
        let mut highest = BTreeMap::new();
        for (module, version, digest) in applied {
            let Some(migration) = known.get(&(module.as_str(), version)) else {
                return Err(Error::ChangedOrUnknownMigration);
            };
            let expected = Sha256::digest(migration.sql.as_bytes());
            if digest.as_slice() != expected.as_slice() {
                return Err(Error::ChangedOrUnknownMigration);
            }
            let previous = highest.entry(module).or_insert(0);
            if version != *previous + 1 {
                return Err(Error::ChangedOrUnknownMigration);
            }
            *previous = version;
        }
        for ((module, version), migration) in known {
            if version > *highest.get(module).unwrap_or(&0) {
                tx.execute_batch(migration.sql)?;
                tx.execute(
                    "INSERT INTO core_migrations VALUES (?1,?2,?3)",
                    rusqlite::params![
                        module,
                        version,
                        Sha256::digest(migration.sql.as_bytes()).as_slice()
                    ],
                )?;
            }
        }
        tx.commit()?;
        Ok(Self { connection })
    }

    /// Execute module-owned work and its durable receipt in the same transaction.
    /// The namespace binds the authenticated actor/action/resource context; canonical
    /// request bytes must bind every relevant argument. Modules own both encodings.
    /// Work uses this existing transaction and must not begin/end transactions itself.
    pub fn transaction(
        &mut self,
        command: Command<'_>,
        work: impl FnOnce(&Transaction<'_>) -> Result<Vec<u8>>,
    ) -> Result<Receipt> {
        if !valid_key(command.namespace) || !valid_key(command.id) {
            return Err(Error::InvalidCommandKey);
        }
        let digest = Sha256::digest(command.request);
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let old:Option<(Vec<u8>,Vec<u8>)>=tx.query_row("SELECT request_hash,response FROM core_command_receipts WHERE namespace=?1 AND command_id=?2",[command.namespace,command.id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((old_digest, bytes)) = old {
            if old_digest.as_slice() != digest.as_slice() {
                return Err(Error::ReusedCommandId);
            }
            tx.commit()?;
            return Ok(Receipt {
                bytes,
                replayed: true,
            });
        }
        let bytes = work(&tx)?;
        if bytes.len() > MAX_RECEIPT_BYTES {
            return Err(Error::ReceiptTooLarge);
        }
        tx.execute(
            "INSERT INTO core_command_receipts VALUES (?1,?2,?3,?4)",
            rusqlite::params![command.namespace, command.id, digest.as_slice(), bytes],
        )?;
        tx.commit()?;
        Ok(Receipt {
            bytes,
            replayed: false,
        })
    }
}

/// Independent read-only connection. Modules own queries; games never receive this handle.
pub fn open_reader(path: impl AsRef<Path>) -> Result<Connection> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let application: i32 = connection.pragma_query_value(None, "application_id", |r| r.get(0))?;
    let version: i32 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if application != APPLICATION_ID {
        return Err(Error::ForeignDatabase);
    }
    if version != SCHEMA_VERSION {
        return Err(Error::UnsupportedSchema(version));
    }
    configure(&connection, false)?;
    Ok(connection)
}
