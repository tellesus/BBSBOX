// SPDX-License-Identifier: AGPL-3.0-or-later
//! Invitation consent and management on the group's home node.
use super::{Error, GroupId, Kind, PAGE_SIZE, Role, active, get, role};
use bbsbox_credentials::NodeId;
use bbsbox_identity::{LocalAccountId, account_by_username};
use rand_core::{OsRng, RngCore};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

pub(crate) fn invitation(
    connection: &Connection,
    group: GroupId,
    account: LocalAccountId,
) -> rusqlite::Result<Option<[u8; 16]>> {
    connection
        .query_row(
            "SELECT generation FROM groups_invites WHERE group_id=?1 AND account=?2",
            params![group.0.as_slice(), account.bytes().as_slice()],
            |row| row.get(0),
        )
        .optional()
}

pub fn invite(
    tx: &Transaction<'_>,
    home: NodeId,
    group: GroupId,
    actor: LocalAccountId,
    username: &str,
) -> Result<(), Error> {
    active(tx, actor)?;
    get(tx, home, group)?.ok_or(Error::Missing)?;
    if !role(tx, group, actor)?.is_some_and(Role::can_manage) {
        return Err(Error::Forbidden);
    }
    let recipient = account_by_username(tx, username).map_err(Error::Identity)?;
    if !recipient.active || role(tx, group, recipient.id)?.is_some() {
        return Err(Error::Conflict);
    }
    let mut generation = [0; 16];
    OsRng.fill_bytes(&mut generation);
    tx.execute(
        "INSERT OR IGNORE INTO groups_invites VALUES (?1,?2,?3,?4)",
        params![
            group.0.as_slice(),
            recipient.id.bytes().as_slice(),
            actor.bytes().as_slice(),
            generation.as_slice()
        ],
    )?;
    Ok(())
}

pub struct Join {
    pub generation: [u8; 16],
    pub displayed_profile_revision: i64,
    pub accept: bool,
}

pub fn decide(
    tx: &Transaction<'_>,
    home: NodeId,
    group: GroupId,
    actor: LocalAccountId,
    join: Join,
) -> Result<(), Error> {
    active(tx, actor)?;
    let current = get(tx, home, group)?.ok_or(Error::Missing)?;
    let generation = invitation(tx, group, actor)?.ok_or(Error::Missing)?;
    if generation != join.generation
        || (join.accept && current.revision != join.displayed_profile_revision)
    {
        return Err(Error::Conflict);
    }
    if join.accept {
        tx.execute(
            "INSERT INTO groups_members VALUES (?1,?2,'member')",
            params![group.0.as_slice(), actor.bytes().as_slice()],
        )?;
        advance(tx, group, current.membership_revision)?;
    }
    tx.execute(
        "DELETE FROM groups_invites WHERE group_id=?1 AND account=?2",
        params![group.0.as_slice(), actor.bytes().as_slice()],
    )?;
    Ok(())
}

pub enum Management<'a> {
    Promote(&'a str),
    Handover(&'a str),
    Leave,
    Dissolve,
}

pub fn manage(
    tx: &Transaction<'_>,
    home: NodeId,
    group: GroupId,
    actor: LocalAccountId,
    expected: i64,
    change: Management<'_>,
) -> Result<(), Error> {
    active(tx, actor)?;
    let current = get(tx, home, group)?.ok_or(Error::Missing)?;
    let actor_role = role(tx, group, actor)?.ok_or(Error::Forbidden)?;
    if !actor_role.can_manage() && !matches!(change, Management::Leave) {
        return Err(Error::Forbidden);
    }
    if current.membership_revision != expected {
        return Err(Error::Conflict);
    }
    match change {
        Management::Promote(username) | Management::Handover(username) => {
            let target = account_by_username(tx, username).map_err(Error::Identity)?;
            let target_role = role(tx, group, target.id)?.ok_or(Error::Missing)?;
            if !target.active || target.id == actor {
                return Err(Error::Conflict);
            }
            let handover = matches!(change, Management::Handover(_));
            if !target_role.can_manage() {
                change_role(tx, group, target.id, "manager")?;
            } else if !handover {
                return Err(Error::Conflict);
            }
            if handover {
                change_role(tx, group, actor, "member")?;
            }
        }
        Management::Leave => {
            if actor_role.can_manage() && active_managers(tx, group)? == 1 {
                return Err(Error::Conflict);
            }
            tx.execute(
                "DELETE FROM groups_members WHERE group_id=?1 AND account=?2",
                params![group.0.as_slice(), actor.bytes().as_slice()],
            )?;
        }
        Management::Dissolve => {
            if active_managers(tx, group)? != 1 {
                return Err(Error::Conflict);
            }
            tx.execute(
                "UPDATE groups_local SET dissolved=1 WHERE id=?1",
                [group.0.as_slice()],
            )?;
            tx.execute(
                "DELETE FROM groups_invites WHERE group_id=?1",
                [group.0.as_slice()],
            )?;
        }
    }
    advance(tx, group, current.membership_revision)?;
    Ok(())
}

fn active_managers(tx: &Transaction<'_>, group: GroupId) -> rusqlite::Result<i64> {
    tx.query_row(
        "SELECT count(*) FROM groups_members m JOIN identity_accounts a ON a.id=m.account
           WHERE m.group_id=?1 AND m.role IN ('creator','manager') AND a.active=1",
        [group.0.as_slice()],
        |row| row.get(0),
    )
}

fn change_role(
    tx: &Transaction<'_>,
    group: GroupId,
    account: LocalAccountId,
    role: &str,
) -> rusqlite::Result<()> {
    tx.execute(
        "UPDATE groups_members SET role=?3 WHERE group_id=?1 AND account=?2",
        params![group.0.as_slice(), account.bytes().as_slice(), role],
    )?;
    Ok(())
}

fn advance(tx: &Transaction<'_>, group: GroupId, previous: i64) -> Result<(), Error> {
    let revision = previous.checked_add(1).ok_or(Error::Conflict)?;
    tx.execute(
        "UPDATE groups_local SET membership_revision=?2 WHERE id=?1",
        params![group.0.as_slice(), revision],
    )?;
    Ok(())
}

pub struct Invitation {
    pub group: GroupId,
    pub name: String,
    pub sequence: i64,
    pub invited_by: String,
}

pub fn received(
    connection: &Connection,
    home: NodeId,
    actor: LocalAccountId,
    before: Option<i64>,
) -> rusqlite::Result<Vec<Invitation>> {
    let mut query = connection.prepare(
        "SELECT g.id,g.name,g.sequence,sender.username
           FROM groups_invites i JOIN groups_local g ON g.id=i.group_id
           JOIN identity_accounts sender ON sender.id=i.invited_by
           WHERE i.account=?1 AND g.home=?2 AND g.dissolved=0 AND (?3 IS NULL OR g.sequence<?3)
           ORDER BY g.sequence DESC LIMIT 51",
    )?;
    query
        .query_map(
            params![actor.bytes().as_slice(), home.bytes().as_slice(), before],
            |row| {
                Ok(Invitation {
                    group: GroupId(row.get(0)?),
                    name: row.get(1)?,
                    sequence: row.get(2)?,
                    invited_by: row.get(3)?,
                })
            },
        )?
        .collect()
}

/// For an authorized local game adapter. A reference supplied by a game never creates membership.
pub fn local_role(
    connection: &Connection,
    home: NodeId,
    group: GroupId,
    account: LocalAccountId,
) -> rusqlite::Result<Option<Role>> {
    connection
        .query_row(
            "SELECT m.role FROM groups_members m JOIN groups_local g ON g.id=m.group_id
         JOIN identity_accounts a ON a.id=m.account
         WHERE g.id=?1 AND g.home=?2 AND g.dissolved=0 AND m.account=?3 AND a.active=1",
            params![
                group.0.as_slice(),
                home.bytes().as_slice(),
                account.bytes().as_slice()
            ],
            |row| {
                let key: String = row.get(0)?;
                Role::from_database(&key)
            },
        )
        .optional()
}

pub struct LocalMembership {
    pub group: GroupId,
    pub name: String,
    pub kind: Kind,
    pub role: Role,
    pub sequence: i64,
}

/// List only the account's own active memberships for a scoped local game session.
pub fn local_memberships(
    connection: &Connection,
    home: NodeId,
    account: LocalAccountId,
    before: Option<i64>,
) -> rusqlite::Result<Vec<LocalMembership>> {
    let mut query = connection.prepare(
        "SELECT g.id,g.name,g.kind,m.role,g.sequence FROM groups_members m
         JOIN groups_local g ON g.id=m.group_id JOIN identity_accounts a ON a.id=m.account
         WHERE m.account=?1 AND a.active=1 AND g.home=?2 AND g.dissolved=0
           AND (?3 IS NULL OR g.sequence<?3)
         ORDER BY g.sequence DESC LIMIT ?4",
    )?;
    query
        .query_map(
            params![
                account.bytes().as_slice(),
                home.bytes().as_slice(),
                before,
                (PAGE_SIZE + 1) as i64
            ],
            |row| {
                let kind: String = row.get(2)?;
                let role: String = row.get(3)?;
                Ok(LocalMembership {
                    group: GroupId(row.get(0)?),
                    name: row.get(1)?,
                    kind: Kind::parse(&kind).ok_or(rusqlite::Error::InvalidQuery)?,
                    role: Role::from_database(&role)?,
                    sequence: row.get(4)?,
                })
            },
        )?
        .collect()
}
