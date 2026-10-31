// SPDX-License-Identifier: AGPL-3.0-or-later
//! M02: bounded writer, atomic commands/jobs, local blobs, disk admission and core snapshots.
use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

pub mod blobs;
pub mod clock;
pub mod disk;
pub mod jobs;
pub mod snapshot;
pub mod writer;

const APPLICATION_ID: i32 = 0x42425358;
const SCHEMA_VERSION: i32 = 3;
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
    Io(std::io::Error),
    WriterAlreadyRunning,
    QueueFull,
    WriterClosed,
    ReentrantWriter,
    WorkPanicked,
    InvalidConfiguration,
    InvalidJob,
    ReusedJobId,
    StaleClaim,
    DiskPressure { available: u64, needed: u64 },
    BlobTooLarge,
    InvalidBlob,
    CorruptBlob,
    InvalidSnapshot,
    RecoveryRequired,
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
            Self::Io(error) => write!(f, "storage I/O failed: {error}"),
            Self::WriterAlreadyRunning => f.write_str("a writer already owns this database"),
            Self::QueueFull => f.write_str("the bounded writer queue is full"),
            Self::WriterClosed => f.write_str("the writer is closed"),
            Self::ReentrantWriter => f.write_str("writer work cannot submit more writer work"),
            Self::WorkPanicked => {
                f.write_str("writer work panicked; consult durable receipts before retrying")
            }
            Self::InvalidConfiguration => {
                f.write_str("invalid storage configuration or clock value")
            }
            Self::InvalidJob => f.write_str("invalid job parameters"),
            Self::ReusedJobId => {
                f.write_str("job idempotency key was reused with different arguments")
            }
            Self::StaleClaim => f.write_str("the job claim is expired or has been replaced"),
            Self::DiskPressure { available, needed } => write!(
                f,
                "storage pressure: {available} bytes available; {needed} required"
            ),
            Self::BlobTooLarge => f.write_str("the file exceeds the configured size limit"),
            Self::InvalidBlob => f.write_str("invalid or missing blob/reference"),
            Self::CorruptBlob => f.write_str("blob contents do not match their address"),
            Self::InvalidSnapshot => f.write_str("snapshot is incomplete, corrupt or unsupported"),
            Self::RecoveryRequired => {
                f.write_str("restored storage requires recovery before starting a writer")
            }
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sqlite(error) => Some(error),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}
impl From<rusqlite::Error> for Error {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sqlite(value)
    }
}
impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

/// All compiled module migrations remain registered even when a module is disabled.
#[derive(Clone)]
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
/// Node modules access it through writer::Writer; direct opens are for bootstrap/tests.
/// This crate is not yet integrated into the public HTTP prototype.
pub struct Database {
    connection: Connection,
    clock: Arc<dyn clock::Clock>,
    path: PathBuf,
    budget: disk::Budget,
    space: Arc<dyn disk::SpaceProbe>,
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
        Self::open_with_clock(path, migrations, Arc::new(clock::SystemClock::default()))
    }
    pub fn open_with_clock(
        path: impl AsRef<Path>,
        migrations: &[Migration],
        clock: Arc<dyn clock::Clock>,
    ) -> Result<Self> {
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
        let mut connection = Connection::open(&path)?;
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
        if !fresh && !(1..=SCHEMA_VERSION).contains(&version) {
            return Err(Error::UnsupportedSchema(version));
        }
        configure(&connection, true)?;
        let tx = connection.transaction()?;
        if fresh {
            tx.execute_batch("CREATE TABLE core_migrations(module TEXT NOT NULL,version INTEGER NOT NULL,digest BLOB NOT NULL,PRIMARY KEY(module,version));
                CREATE TABLE core_command_receipts(namespace TEXT NOT NULL,command_id TEXT NOT NULL,request_hash BLOB NOT NULL,response BLOB NOT NULL,PRIMARY KEY(namespace,command_id));")?;
            tx.pragma_update(None, "application_id", APPLICATION_ID)?;
        }
        if fresh || version == 1 {
            tx.execute_batch(jobs::SCHEMA)?;
        }
        if fresh || version < 3 {
            tx.execute_batch(blobs::SCHEMA)?;
        }
        if version != SCHEMA_VERSION {
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
        Ok(Self {
            connection,
            clock,
            path: path.as_ref().canonicalize()?,
            budget: disk::Budget::default(),
            space: Arc::new(disk::FilesystemSpace),
        })
    }

    /// Set once during service initialization. Injectable probe permits pressure tests without filling a disk.
    pub fn configure_disk(
        &mut self,
        budget: disk::Budget,
        probe: Arc<dyn disk::SpaceProbe>,
    ) -> Result<()> {
        self.budget = budget.validate()?;
        self.space = probe;
        Ok(())
    }
    fn directory(&self) -> &Path {
        self.path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
    }
    pub(crate) fn admit(&self, class: disk::Class, growth: u64) -> Result<()> {
        self.budget
            .admit(self.space.as_ref(), self.directory(), class, growth)
    }
    pub(crate) fn admission(&self, class: disk::Class) -> disk::Admission {
        disk::Admission {
            budget: self.budget,
            probe: self.space.clone(),
            path: self.directory().to_owned(),
            class,
        }
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
        self.transaction_with_budget(command, disk::Class::Content, 64 * 1024, work)
    }

    /// Trusted module services select a class and conservative growth estimate, including WAL/receipts.
    pub fn transaction_with_budget(
        &mut self,
        command: Command<'_>,
        class: disk::Class,
        expected_growth: u64,
        work: impl FnOnce(&Transaction<'_>) -> Result<Vec<u8>>,
    ) -> Result<Receipt> {
        self.transaction_service(command, class, expected_growth, work)
    }

    pub fn transaction_service<E: From<Error>>(
        &mut self,
        command: Command<'_>,
        class: disk::Class,
        expected_growth: u64,
        work: impl FnOnce(&Transaction<'_>) -> std::result::Result<Vec<u8>, E>,
    ) -> std::result::Result<Receipt, E> {
        if !valid_key(command.namespace) || !valid_key(command.id) {
            return Err(Error::InvalidCommandKey.into());
        }
        let directory = self.directory().to_owned();
        let digest = Sha256::digest(command.request);
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(Error::from)?;
        let old:Option<(Vec<u8>,Vec<u8>)>=tx.query_row("SELECT request_hash,response FROM core_command_receipts WHERE namespace=?1 AND command_id=?2",[command.namespace,command.id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(Error::from)?;
        if let Some((old_digest, bytes)) = old {
            if old_digest.as_slice() != digest.as_slice() {
                return Err(Error::ReusedCommandId.into());
            }
            tx.commit().map_err(Error::from)?;
            return Ok(Receipt {
                bytes,
                replayed: true,
            });
        }
        self.budget
            .admit(self.space.as_ref(), &directory, class, expected_growth)?;
        let bytes = work(&tx)?;
        if bytes.len() > MAX_RECEIPT_BYTES {
            return Err(Error::ReceiptTooLarge.into());
        }
        tx.execute(
            "INSERT INTO core_command_receipts VALUES (?1,?2,?3,?4)",
            rusqlite::params![command.namespace, command.id, digest.as_slice(), bytes],
        )
        .map_err(Error::from)?;
        self.budget
            .admit(self.space.as_ref(), &directory, class, 0)?;
        tx.commit().map_err(Error::from)?;
        Ok(Receipt {
            bytes,
            replayed: false,
        })
    }

    /// Typed service transaction for operations such as login. Never place session secrets in command receipts.
    pub fn write<T, E: From<Error>>(
        &mut self,
        class: disk::Class,
        expected_growth: u64,
        work: impl FnOnce(&Transaction<'_>) -> std::result::Result<T, E>,
    ) -> std::result::Result<T, E> {
        let guard = self.admission(class);
        guard.check(expected_growth)?;
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(Error::from)?;
        let result = work(&tx)?;
        guard.check(0)?;
        tx.commit().map_err(Error::from)?;
        Ok(result)
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
