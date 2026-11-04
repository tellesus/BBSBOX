// SPDX-License-Identifier: AGPL-3.0-or-later
//! Local clan and cohort directory and roster preferences.
use bbsbox_credentials::NodeId;
use bbsbox_identity::{LocalAccountId, account, normalize_username};
use bbsbox_storage::Migration;
use rand_core::{OsRng, RngCore};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

pub const PAGE_SIZE: usize = 50;

pub fn migrations() -> [Migration; 1] {
    [Migration {
        module: "groups",
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
pub struct GroupId([u8; 16]);

impl GroupId {
    pub fn parse(value: &str) -> Option<Self> {
        if value.len() != 32
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return None;
        }
        let mut id = [0; 16];
        hex::decode_to_slice(value, &mut id).ok()?;
        Some(Self(id))
    }
}

impl std::fmt::Display for GroupId {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        output.write_str(&hex::encode(self.0))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Clan,
    Cohort,
}

impl Kind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "clan" => Some(Self::Clan),
            "cohort" => Some(Self::Cohort),
            _ => None,
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Clan => "clan",
            Self::Cohort => "cohort",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Creator,
    Manager,
    Member,
}

impl Role {
    fn from_database(value: &str) -> rusqlite::Result<Self> {
        match value {
            "creator" => Ok(Self::Creator),
            "manager" => Ok(Self::Manager),
            "member" => Ok(Self::Member),
            _ => Err(rusqlite::Error::InvalidQuery),
        }
    }
    pub fn can_manage(self) -> bool {
        matches!(self, Self::Creator | Self::Manager)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Details {
    pub name: String,
    pub description: String,
    pub public_roster: bool,
}

impl Details {
    fn valid(&self) -> bool {
        !self.name.trim().is_empty()
            && self.name.len() <= 120
            && !self.name.chars().any(char::is_control)
            && self.description.len() <= 8 * 1024
            && !self
                .description
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\t'))
    }
}

#[derive(Debug)]
pub struct Group {
    pub id: GroupId,
    pub kind: Kind,
    pub details: Details,
    pub revision: i64,
}

fn group(row: &rusqlite::Row<'_>) -> rusqlite::Result<Group> {
    let kind: String = row.get(1)?;
    Ok(Group {
        id: GroupId(row.get(0)?),
        kind: Kind::parse(&kind).ok_or(rusqlite::Error::InvalidQuery)?,
        details: Details {
            name: row.get(2)?,
            description: row.get(3)?,
            public_roster: row.get(4)?,
        },
        revision: row.get(5)?,
    })
}

pub fn get(connection: &Connection, home: NodeId, id: GroupId) -> rusqlite::Result<Option<Group>> {
    connection.query_row(
        "SELECT id,kind,name,description,public_roster,revision FROM groups_local WHERE id=?1 AND home=?2",
        params![id.0.as_slice(), home.bytes().as_slice()], group,
    ).optional()
}

pub struct DirectoryEntry {
    pub id: GroupId,
    pub kind: Kind,
    pub name: String,
    pub sequence: i64,
}

pub fn directory(
    connection: &Connection,
    home: NodeId,
    kind: Option<Kind>,
    before: Option<i64>,
) -> rusqlite::Result<Vec<DirectoryEntry>> {
    let mut query = connection.prepare(
        "SELECT id,kind,name,sequence FROM groups_local WHERE home=?1 AND (?2 IS NULL OR kind=?2)
           AND (?3 IS NULL OR sequence<?3) ORDER BY sequence DESC LIMIT ?4",
    )?;
    query
        .query_map(
            params![
                home.bytes().as_slice(),
                kind.map(Kind::key),
                before,
                (PAGE_SIZE + 1) as i64
            ],
            |row| {
                let kind: String = row.get(1)?;
                Ok(DirectoryEntry {
                    id: GroupId(row.get(0)?),
                    kind: Kind::parse(&kind).ok_or(rusqlite::Error::InvalidQuery)?,
                    name: row.get(2)?,
                    sequence: row.get(3)?,
                })
            },
        )?
        .collect()
}

/// The runtime supplies the local node identity and account authenticated in this transaction.
pub fn create(
    tx: &Transaction<'_>,
    home: NodeId,
    actor: LocalAccountId,
    kind: Kind,
    details: &Details,
) -> Result<GroupId, Error> {
    active(tx, actor)?;
    if !details.valid() {
        return Err(Error::InvalidInput);
    }
    let mut id = [0; 16];
    OsRng.fill_bytes(&mut id);
    tx.execute(
        "INSERT INTO groups_local(id,home,kind,created_by,name,description,public_roster,revision)
         VALUES (?1,?2,?3,?4,?5,?6,?7,1)",
        params![
            id.as_slice(),
            home.bytes().as_slice(),
            kind.key(),
            actor.bytes().as_slice(),
            details.name,
            details.description,
            details.public_roster
        ],
    )?;
    tx.execute(
        "INSERT INTO groups_members VALUES (?1,?2,'creator')",
        params![id.as_slice(), actor.bytes().as_slice()],
    )?;
    Ok(GroupId(id))
}

pub fn edit(
    tx: &Transaction<'_>,
    home: NodeId,
    actor: LocalAccountId,
    id: GroupId,
    expected: i64,
    details: &Details,
) -> Result<(), Error> {
    active(tx, actor)?;
    let current = get(tx, home, id)?.ok_or(Error::Missing)?;
    if !role(tx, id, actor)?.is_some_and(Role::can_manage) {
        return Err(Error::Forbidden);
    }
    if current.revision != expected {
        return Err(Error::Conflict);
    }
    if !details.valid() {
        return Err(Error::InvalidInput);
    }
    if current.details == *details {
        return Ok(());
    }
    let revision = expected.checked_add(1).ok_or(Error::Conflict)?;
    tx.execute(
        "UPDATE groups_local SET name=?2,description=?3,public_roster=?4,revision=?5 WHERE id=?1",
        params![
            id.0.as_slice(),
            details.name,
            details.description,
            details.public_roster,
            revision
        ],
    )?;
    Ok(())
}

fn active(connection: &Connection, id: LocalAccountId) -> Result<(), Error> {
    if account(connection, id).map_err(Error::Identity)?.active {
        Ok(())
    } else {
        Err(Error::Forbidden)
    }
}

fn role(
    connection: &Connection,
    id: GroupId,
    account: LocalAccountId,
) -> rusqlite::Result<Option<Role>> {
    connection
        .query_row(
            "SELECT role FROM groups_members WHERE group_id=?1 AND account=?2",
            params![id.0.as_slice(), account.bytes().as_slice()],
            |row| {
                let role: String = row.get(0)?;
                Role::from_database(&role)
            },
        )
        .optional()
}

pub struct RosterEntry {
    pub username: String,
    pub role: Role,
}

pub struct View {
    pub group: Group,
    pub role: Option<Role>,
    pub roster: Option<Vec<RosterEntry>>,
}

pub fn view(
    tx: &Transaction<'_>,
    home: NodeId,
    id: GroupId,
    viewer: LocalAccountId,
    after: Option<&str>,
) -> Result<View, Error> {
    active(tx, viewer)?;
    let group = get(tx, home, id)?.ok_or(Error::Missing)?;
    let membership = role(tx, id, viewer)?;
    let roster = if group.details.public_roster || membership.is_some() {
        let after = after
            .map(normalize_username)
            .transpose()
            .map_err(|_| Error::InvalidInput)?;
        let mut query = tx.prepare(
            "SELECT a.username,m.role FROM groups_members m JOIN identity_accounts a ON a.id=m.account
             WHERE m.group_id=?1 AND a.active=1 AND a.username>coalesce(?2,'') ORDER BY a.username LIMIT ?3",
        )?;
        Some(
            query
                .query_map(
                    params![id.0.as_slice(), after, (PAGE_SIZE + 1) as i64],
                    |row| {
                        let role: String = row.get(1)?;
                        Ok(RosterEntry {
                            username: row.get(0)?,
                            role: Role::from_database(&role)?,
                        })
                    },
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?,
        )
    } else {
        None
    };
    Ok(View {
        group,
        role: membership,
        roster,
    })
}
