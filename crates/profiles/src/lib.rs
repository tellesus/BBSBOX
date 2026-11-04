// SPDX-License-Identifier: AGPL-3.0-or-later
//! Local profile preferences and server-filtered public views.
use bbsbox_identity::LocalAccountId;
use bbsbox_storage::Migration;
use rusqlite::{Connection, OptionalExtension, Transaction, params};

pub mod components;
use components::{Display, Providers, Selection, Viewer};

pub fn migrations() -> [Migration; 2] {
    [
        Migration {
            module: "profiles",
            version: 1,
            sql: include_str!("schema.sql"),
        },
        Migration {
            module: "profiles",
            version: 2,
            sql: include_str!("components.sql"),
        },
    ]
}

#[derive(Debug)]
pub enum Error {
    InvalidField,
    Conflict,
    Identity(bbsbox_identity::Error),
    Storage(bbsbox_storage::Error),
    Sqlite(rusqlite::Error),
}

impl From<bbsbox_storage::Error> for Error {
    fn from(error: bbsbox_storage::Error) -> Self {
        Self::Storage(error)
    }
}

impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Field {
    pub text: String,
    pub publish: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Preferences {
    pub public: bool,
    pub bio: Field,
    pub message: Field,
    pub email: Field,
    pub phone: Field,
    pub contact: Field,
    pub components: Selection,
}

#[derive(Debug, Default)]
pub struct Editor {
    pub revision: i64,
    pub preferences: Preferences,
}

fn field(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Field> {
    Ok(Field {
        text: row.get(index)?,
        publish: row.get(index + 5)?,
    })
}

/// The caller supplies the authenticated owner's account ID.
pub fn editor(connection: &Connection, account: LocalAccountId) -> Result<Editor, Error> {
    Ok(connection.query_row(
        "SELECT revision,public,bio,message,email,phone,contact,show_bio,show_message,show_email,show_phone,show_contact,show_feed,show_events,show_games,show_drops FROM profiles_local WHERE account=?1",
        [account.bytes().as_slice()],
        |row| Ok(Editor {
            revision: row.get(0)?,
            preferences: Preferences {
                public: row.get(1)?,
                bio: field(row, 2)?,
                message: field(row, 3)?,
                email: field(row, 4)?,
                phone: field(row, 5)?,
                contact: field(row, 6)?,
                components: Selection::read(row, 12)?,
            },
        }),
    ).optional()?.unwrap_or_default())
}

/// Use the same transaction that authorizes a mutation by the profile owner.
pub fn save(
    tx: &Transaction<'_>,
    account: LocalAccountId,
    expected: i64,
    value: &Preferences,
) -> Result<(), Error> {
    let current = editor(tx, account)?;
    if current.revision != expected {
        return Err(Error::Conflict);
    }
    for (field, limit, multiline) in [
        (&value.bio, 4096, true),
        (&value.message, 280, false),
        (&value.email, 254, false),
        (&value.phone, 64, false),
        (&value.contact, 512, true),
    ] {
        if field.text.len() > limit
            || field
                .text
                .chars()
                .any(|c| c.is_control() && !(multiline && matches!(c, '\n' | '\t')))
        {
            return Err(Error::InvalidField);
        }
    }
    if current.preferences == *value {
        return Ok(());
    }
    let revision = expected.checked_add(1).ok_or(Error::Conflict)?;
    tx.execute(
        "INSERT INTO profiles_local (account,revision,public,bio,message,email,phone,contact,
         show_bio,show_message,show_email,show_phone,show_contact,show_feed,show_events,show_games,show_drops)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)
         ON CONFLICT(account) DO UPDATE SET revision=excluded.revision,public=excluded.public,
         bio=excluded.bio,message=excluded.message,email=excluded.email,phone=excluded.phone,contact=excluded.contact,
         show_bio=excluded.show_bio,show_message=excluded.show_message,show_email=excluded.show_email,show_phone=excluded.show_phone,show_contact=excluded.show_contact,
         show_feed=excluded.show_feed,show_events=excluded.show_events,show_games=excluded.show_games,show_drops=excluded.show_drops",
        params![
            account.bytes().as_slice(), revision, value.public,
            value.bio.text, value.message.text, value.email.text, value.phone.text, value.contact.text,
            value.bio.publish, value.message.publish, value.email.publish, value.phone.publish, value.contact.publish,
            value.components.feed, value.components.events, value.components.games, value.components.drops,
        ],
    )?;
    Ok(())
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct PublishedFields {
    pub bio: Option<String>,
    pub message: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub contact: Option<String>,
}

pub struct Profile {
    pub username: String,
    pub fields: PublishedFields,
    pub components: Display,
}

/// Private values never leave the public query, even for a caller with a SysOp session.
pub fn profile(
    connection: &Transaction<'_>,
    username: &str,
    viewer: LocalAccountId,
    providers: &Providers,
) -> Result<Profile, Error> {
    let account =
        bbsbox_identity::account_by_username(connection, username).map_err(Error::Identity)?;
    let (fields, public, selection) = connection
        .query_row(
            "SELECT CASE WHEN show_bio AND public THEN nullif(trim(bio),'') END,
                CASE WHEN show_message AND public THEN nullif(trim(message),'') END,
                CASE WHEN show_email AND public THEN nullif(trim(email),'') END,
                CASE WHEN show_phone AND public THEN nullif(trim(phone),'') END,
                CASE WHEN show_contact AND public THEN nullif(trim(contact),'') END,
                public,show_feed,show_events,show_games,show_drops
           FROM profiles_local WHERE account=?1",
            [account.id.bytes().as_slice()],
            |row| {
                Ok((
                    PublishedFields {
                        bio: row.get(0)?,
                        message: row.get(1)?,
                        email: row.get(2)?,
                        phone: row.get(3)?,
                        contact: row.get(4)?,
                    },
                    row.get(5)?,
                    Selection::read(row, 6)?,
                ))
            },
        )
        .optional()?
        .unwrap_or_default();
    Ok(Profile {
        username: account.username,
        fields,
        components: providers.read(
            connection,
            Viewer {
                account: viewer,
                profile: account.id,
            },
            public,
            selection,
        )?,
    })
}
