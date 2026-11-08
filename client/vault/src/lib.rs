// SPDX-License-Identifier: AGPL-3.0-or-later
//! One locked client writer. SQLite receives only complete encrypted identity/state checkpoints.
use bbsbox_client_identity::{
    local::{Opened, Session},
    recovery::{self, Engine},
};
use bbsbox_credentials::{Credential, PersonId, Purposes};
use bbsbox_storage::{
    self as storage, Migration,
    disk::Class,
    writer::{Priority, Writer},
};
use rusqlite::{OptionalExtension, params};
use std::{fmt, path::Path};
use zeroize::Zeroizing;

#[derive(Debug)]
pub enum Error {
    Storage(storage::Error),
    Sqlite(rusqlite::Error),
    Crypto(recovery::Error),
    Uninitialized,
    AlreadyInitialized,
    RevisionChanged,
    InvalidRecord,
    InvalidPath,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Storage(storage::Error::WriterAlreadyRunning) => {
                "This identity is already open in another client app"
            }
            Self::Storage(_) => "Client storage is unavailable",
            Self::Sqlite(_) => "Client storage transaction failed",
            Self::Crypto(recovery::Error::WeakPassphrase) => {
                "Use a passphrase of at least 15 characters"
            }
            Self::Crypto(_) => "Client passphrase or encrypted identity is invalid",
            Self::Uninitialized => "Client identity has not been initialized",
            Self::AlreadyInitialized => "Client identity already exists",
            Self::RevisionChanged => "Client state changed before this save",
            Self::InvalidRecord => "Client identity record is invalid",
            Self::InvalidPath => "Use a regular client storage file",
        })
    }
}
impl std::error::Error for Error {}
impl From<storage::Error> for Error {
    fn from(error: storage::Error) -> Self {
        Self::Storage(error)
    }
}
impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}
impl From<recovery::Error> for Error {
    fn from(error: recovery::Error) -> Self {
        Self::Crypto(error)
    }
}

type Result<T> = std::result::Result<T, Error>;

pub struct Vault {
    writer: Writer,
    session: Session,
    revision: u64,
    state: Zeroizing<Vec<u8>>,
}

fn start(path: &Path) -> Result<Writer> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.is_file() => return Err(Error::InvalidPath),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(storage::Error::from(error).into()),
        _ => (),
    }
    Ok(Writer::start(
        path,
        vec![Migration {
            module: "client-vault",
            version: 1,
            sql: include_str!("schema.sql"),
        }],
        storage::writer::Config {
            capacity: 4,
            ..Default::default()
        },
    )?)
}

fn initialized(writer: &Writer) -> Result<bool> {
    writer
        .handle()
        .submit_service(Priority::Interactive, |db| {
            db.write(Class::Control, 0, |tx| {
                Ok(
                    tx.query_row("SELECT EXISTS(SELECT 1 FROM client_vault)", [], |row| {
                        row.get(0)
                    })?,
                )
            })
        })?
        .blocking_wait()
}

impl Vault {
    /// Explicit setup can retry an interrupted empty database, but never replaces a saved identity.
    pub fn create(
        path: &Path,
        engine: &Engine,
        pass: Zeroizing<String>,
        purposes: Purposes,
    ) -> Result<Self> {
        let writer = start(path)?;
        if initialized(&writer)? {
            return Err(Error::AlreadyInitialized);
        }
        let opened = Opened {
            session: Session::create(engine, pass, purposes)?,
            revision: 1,
            state: Zeroizing::new(Vec::new()),
        };
        Self::initialize(writer, opened)
    }

    pub fn recover(
        path: &Path,
        engine: &Engine,
        kit: &[u8],
        backup_pass: Zeroizing<String>,
        vault_pass: Zeroizing<String>,
        purposes: Purposes,
    ) -> Result<Self> {
        let writer = start(path)?;
        if initialized(&writer)? {
            return Err(Error::AlreadyInitialized);
        }
        Self::initialize(
            writer,
            Session::recover(engine, kit, backup_pass, vault_pass, purposes)?,
        )
    }

    fn initialize(writer: Writer, opened: Opened) -> Result<Self> {
        let record = opened.session.checkpoint(1, &opened.state)?;
        let growth = record.len() as u64 + 65536;
        writer
            .handle()
            .submit_service(Priority::Interactive, move |db| {
                db.write(Class::Control, growth, |tx| {
                    if tx.execute(
                        "INSERT OR IGNORE INTO client_vault VALUES(1,1,?1)",
                        [record],
                    )? != 1
                    {
                        return Err(Error::AlreadyInitialized);
                    }
                    Ok(())
                })
            })?
            .blocking_wait()?;
        Ok(Self {
            writer,
            session: opened.session,
            revision: 1,
            state: opened.state,
        })
    }

    pub fn unlock(path: &Path, engine: &Engine, pass: Zeroizing<String>) -> Result<Self> {
        if !path.try_exists().map_err(storage::Error::from)? {
            return Err(Error::Uninitialized);
        }
        let writer = start(path)?;
        let (revision,record)=writer.handle().submit_service(Priority::Interactive,|db| {
            db.write(Class::Control,0,|tx| {
                let row=tx.query_row("SELECT revision,CASE WHEN length(record)<=8390000 THEN record END FROM client_vault WHERE id=1",[],
                    |row|Ok((row.get::<_,i64>(0)?,row.get::<_,Option<Vec<u8>>>(1)?))).optional()?;
                let (revision,record)=row.ok_or(Error::Uninitialized)?;
                if revision<=0 {return Err(Error::InvalidRecord);}
                Ok((revision as u64,record.ok_or(Error::InvalidRecord)?))
            })
        })?.blocking_wait()?;
        let opened = Session::open(engine, &record, pass)?;
        if opened.revision != revision {
            return Err(Error::InvalidRecord);
        }
        Ok(Self {
            writer,
            session: opened.session,
            revision,
            state: opened.state,
        })
    }

    /// Run on a blocking client worker. Failed or stale saves leave the unlocked state unchanged.
    pub fn checkpoint(&mut self, expected: u64, state: Zeroizing<Vec<u8>>) -> Result<u64> {
        if expected != self.revision {
            return Err(Error::RevisionChanged);
        }
        let revision = expected
            .checked_add(1)
            .filter(|revision| *revision <= i64::MAX as u64)
            .ok_or(Error::InvalidRecord)?;
        let record = self.session.checkpoint(revision, &state)?;
        let growth = record.len() as u64 + 65536;
        self.writer
            .handle()
            .submit_service(Priority::Interactive, move |db| {
                db.write(Class::Control, growth, |tx| {
                    if tx.execute(
                        "UPDATE client_vault SET revision=?1,record=?2 WHERE id=1 AND revision=?3",
                        params![revision as i64, record, expected as i64],
                    )? != 1
                    {
                        return Err(Error::RevisionChanged);
                    }
                    Ok(())
                })
            })?
            .blocking_wait()?;
        self.revision = revision;
        self.state = state;
        Ok(revision)
    }

    pub fn person_id(&self) -> PersonId {
        self.session.person_id()
    }
    pub fn credential(&self) -> &Credential {
        self.session.device().credential()
    }
    pub fn control_incarnation(&self) -> &[u8; 16] {
        self.session.control_incarnation()
    }
    pub fn requires_rejoin(&self) -> bool {
        self.session.requires_rejoin()
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn state(&self) -> &[u8] {
        &self.state
    }
    pub fn export_recovery(
        &self,
        engine: &Engine,
        pass: Zeroizing<String>,
        receive_state: &[u8],
    ) -> Result<Vec<u8>> {
        Ok(self.session.export_recovery(engine, pass, receive_state)?)
    }
}
