// SPDX-License-Identifier: AGPL-3.0-or-later
//! Local-only file metadata and area policy.
use bbsbox_identity::{Account, LocalAccountId, Role, account};
use bbsbox_storage::{
    Migration,
    blobs::{Info, remove_reference},
};
use rand_core::{OsRng, RngCore};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

pub const BLOB_NAMESPACE: &str = "local-files";
pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_AREA_BYTES: i64 = 1024 * 1024 * 1024;
pub const PAGE_SIZE: usize = 50;

pub fn migrations() -> [Migration; 1] {
    [Migration {
        module: "files",
        version: 1,
        sql: include_str!("schema.sql"),
    }]
}

#[derive(Debug)]
pub enum Error {
    InvalidInput,
    Forbidden,
    Missing,
    Conflict,
    Quota,
    Identity(bbsbox_identity::Error),
    Storage(bbsbox_storage::Error),
    Sqlite(rusqlite::Error),
}

impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}
impl From<bbsbox_storage::Error> for Error {
    fn from(error: bbsbox_storage::Error) -> Self {
        Self::Storage(error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileId([u8; 16]);

impl FileId {
    pub fn generate() -> Self {
        let mut id = [0; 16];
        OsRng.fill_bytes(&mut id);
        Self(id)
    }

    pub fn parse(value: &str) -> Option<Self> {
        if value.len() != 32
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return None;
        }
        let mut bytes = [0; 16];
        hex::decode_to_slice(value, &mut bytes).ok()?;
        Some(Self(bytes))
    }
}

impl std::fmt::Display for FileId {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        output.write_str(&hex::encode(self.0))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Members,
    Sysops,
}

impl Access {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "member" => Some(Self::Members),
            "sysop" => Some(Self::Sysops),
            _ => None,
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Members => "member",
            Self::Sysops => "sysop",
        }
    }
    pub fn allows(self, account: &Account) -> bool {
        account.active && (self == Self::Members || account.role == Role::Sysop)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Policy {
    pub title: String,
    pub download: Access,
    pub upload: Access,
    pub quota: i64,
}

impl Policy {
    fn valid(&self) -> bool {
        !self.title.trim().is_empty()
            && self.title.len() <= 120
            && !self.title.chars().any(char::is_control)
            && (1..=MAX_AREA_BYTES).contains(&self.quota)
            && (self.download != Access::Sysops || self.upload == Access::Sysops)
    }
}

#[derive(Debug)]
pub struct Area {
    pub slug: String,
    pub policy: Policy,
    pub revision: i64,
    pub used: i64,
}

pub fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn access(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Access> {
    let value: String = row.get(index)?;
    Access::parse(&value).ok_or(rusqlite::Error::InvalidQuery)
}

fn area(row: &rusqlite::Row<'_>) -> rusqlite::Result<Area> {
    Ok(Area {
        slug: row.get(0)?,
        policy: Policy {
            title: row.get(1)?,
            download: access(row, 2)?,
            upload: access(row, 3)?,
            quota: row.get(4)?,
        },
        revision: row.get(5)?,
        used: row.get(6)?,
    })
}

pub fn list_areas(
    connection: &Connection,
    actor: LocalAccountId,
    after: Option<&str>,
) -> Result<Vec<Area>, Error> {
    let account = active(connection, actor)?;
    let mut query = connection.prepare(
        "SELECT slug,title,download_role,upload_role,quota,revision,used FROM files_areas
         WHERE (?1 OR download_role='member') AND slug>coalesce(?2,'') ORDER BY slug LIMIT ?3",
    )?;
    Ok(query
        .query_map(
            params![account.role == Role::Sysop, after, (PAGE_SIZE + 1) as i64],
            area,
        )?
        .collect::<rusqlite::Result<_>>()?)
}

pub fn read_area(
    connection: &Connection,
    slug: &str,
    actor: LocalAccountId,
) -> Result<(Area, Account), Error> {
    let account = active(connection, actor)?;
    let area = connection.query_row(
        "SELECT slug,title,download_role,upload_role,quota,revision,used FROM files_areas WHERE slug=?1",
        [slug], area,
    ).optional()?.filter(|area| area.policy.download.allows(&account)).ok_or(Error::Missing)?;
    Ok((area, account))
}

pub fn configure(
    tx: &Transaction<'_>,
    slug: &str,
    actor: LocalAccountId,
    expected: Option<i64>,
    policy: &Policy,
) -> Result<(), Error> {
    let account = active(tx, actor)?;
    if account.role != Role::Sysop {
        return Err(Error::Forbidden);
    }
    if !valid_slug(slug) || !policy.valid() {
        return Err(Error::InvalidInput);
    }
    let changed = match expected {
        None => tx.execute(
            "INSERT OR IGNORE INTO files_areas(slug,title,download_role,upload_role,quota,revision) VALUES(?1,?2,?3,?4,?5,1)",
            params![slug, policy.title, policy.download.key(), policy.upload.key(), policy.quota],
        )?,
        Some(revision) => tx.execute(
            "UPDATE files_areas SET title=?2,download_role=?3,upload_role=?4,quota=?5,revision=revision+1
             WHERE slug=?1 AND revision=?6 AND quota>=used AND ?5>=used AND revision<9223372036854775807",
            params![slug, policy.title, policy.download.key(), policy.upload.key(), policy.quota, revision],
        )?,
    };
    if changed == 1 {
        Ok(())
    } else {
        Err(Error::Conflict)
    }
}

pub fn validate_filename(value: &str) -> bool {
    !value.trim().is_empty()
        && value.trim() == value
        && value.len() <= 240
        && !matches!(value, "." | "..")
        && !value
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
}

pub fn media_type(value: Option<&str>) -> String {
    let kind = value.unwrap_or_default();
    if kind.len() <= 96
        && kind.split_once('/').is_some_and(|(left, right)| {
            [left, right].iter().all(|part| {
                !part.is_empty()
                    && part.bytes().all(|b| {
                        b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.' | b'_')
                    })
            })
        })
    {
        kind.to_ascii_lowercase()
    } else {
        "application/octet-stream".into()
    }
}

pub fn allow_upload(
    connection: &Connection,
    slug: &str,
    actor: LocalAccountId,
) -> Result<(), Error> {
    let (area, account) = read_area(connection, slug, actor)?;
    if area.policy.upload.allows(&account) {
        Ok(())
    } else {
        Err(Error::Forbidden)
    }
}

pub struct Upload<'a> {
    pub id: FileId,
    pub area: &'a str,
    pub filename: &'a str,
    pub media_type: &'a str,
}

/// Publish metadata in the checked blob command that created the protecting reference.
pub fn publish(
    tx: &Transaction<'_>,
    actor: LocalAccountId,
    upload: Upload<'_>,
    info: &Info,
) -> Result<(), Error> {
    allow_upload(tx, upload.area, actor)?;
    if !validate_filename(upload.filename)
        || media_type(Some(upload.media_type)) != upload.media_type
        || info.size > MAX_FILE_BYTES
    {
        return Err(Error::InvalidInput);
    }
    if tx.execute(
        "UPDATE files_areas SET used=used+?2 WHERE slug=?1 AND ?2<=quota-used",
        params![upload.area, info.size as i64],
    )? == 0
    {
        return Err(Error::Quota);
    }
    tx.execute(
        "INSERT INTO files_entries(id,area,owner,filename,media_type,size,digest)
         SELECT ?1,?2,?3,?4,?5,?6,?7 FROM core_blob_refs WHERE namespace=?8 AND ref_key=?9 AND digest=?7",
        params![upload.id.0.as_slice(), upload.area, actor.bytes().as_slice(), upload.filename, upload.media_type, info.size as i64, info.digest, BLOB_NAMESPACE, upload.id.to_string()],
    ).and_then(|inserted| if inserted == 1 { Ok(inserted) } else { Err(rusqlite::Error::QueryReturnedNoRows) })?;
    Ok(())
}

#[derive(Debug)]
pub struct Entry {
    pub id: FileId,
    pub area: String,
    pub owner: [u8; 16],
    pub filename: String,
    pub media_type: String,
    pub size: i64,
    pub digest: String,
    pub sequence: i64,
}

fn entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<Entry> {
    Ok(Entry {
        id: FileId(row.get(0)?),
        area: row.get(1)?,
        owner: row.get(2)?,
        filename: row.get(3)?,
        media_type: row.get(4)?,
        size: row.get(5)?,
        digest: row.get(6)?,
        sequence: row.get(7)?,
    })
}

pub fn list(
    connection: &Connection,
    slug: &str,
    actor: LocalAccountId,
    before: Option<i64>,
) -> Result<Vec<Entry>, Error> {
    read_area(connection, slug, actor)?;
    let mut query = connection.prepare(
        "SELECT id,area,owner,filename,media_type,size,digest,sequence FROM files_entries
         WHERE area=?1 AND (?2 IS NULL OR sequence<?2) ORDER BY sequence DESC LIMIT ?3",
    )?;
    Ok(query
        .query_map(params![slug, before, (PAGE_SIZE + 1) as i64], entry)?
        .collect::<rusqlite::Result<_>>()?)
}

pub fn get(connection: &Connection, id: FileId, actor: LocalAccountId) -> Result<Entry, Error> {
    let entry = connection.query_row(
        "SELECT id,area,owner,filename,media_type,size,digest,sequence FROM files_entries WHERE id=?1",
        [id.0.as_slice()], entry,
    ).optional()?.ok_or(Error::Missing)?;
    read_area(connection, &entry.area, actor)?;
    Ok(entry)
}

pub fn remove(tx: &Transaction<'_>, id: FileId, actor: LocalAccountId) -> Result<String, Error> {
    let entry = get(tx, id, actor)?;
    let account = active(tx, actor)?;
    if actor.bytes() != &entry.owner && account.role != Role::Sysop {
        return Err(Error::Forbidden);
    }
    tx.execute("DELETE FROM files_entries WHERE id=?1", [id.0.as_slice()])?;
    remove_reference(tx, BLOB_NAMESPACE, &id.to_string())?;
    tx.execute(
        "UPDATE files_areas SET used=used-?2 WHERE slug=?1",
        params![entry.area, entry.size],
    )?;
    Ok(entry.area)
}

fn active(connection: &Connection, actor: LocalAccountId) -> Result<Account, Error> {
    let account = account(connection, actor).map_err(Error::Identity)?;
    if account.active {
        Ok(account)
    } else {
        Err(Error::Forbidden)
    }
}
