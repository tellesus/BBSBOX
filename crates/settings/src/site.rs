// SPDX-License-Identifier: AGPL-3.0-or-later
//! Local welcome material and menu order.
use super::Error;
use rusqlite::{Connection, OptionalExtension, Transaction, params};

pub const MENUS: [(&str, &str); 6] = [
    ("home,account,status", "Home, Account, Status"),
    ("home,status,account", "Home, Status, Account"),
    ("account,home,status", "Account, Home, Status"),
    ("account,status,home", "Account, Status, Home"),
    ("status,home,account", "Status, Home, Account"),
    ("status,account,home", "Status, Account, Home"),
];

#[derive(Clone, PartialEq, Eq)]
pub struct Site {
    pub revision: i64,
    pub welcome: String,
    pub artwork: String,
    menu: &'static str,
}

impl Default for Site {
    fn default() -> Self {
        Self {
            revision: 0,
            welcome: String::new(),
            artwork: String::new(),
            menu: MENUS[0].0,
        }
    }
}

impl Site {
    pub fn new(revision: i64, welcome: &str, artwork: &str, menu: &str) -> Option<Self> {
        let menu = MENUS.iter().find(|(id, _)| *id == menu)?.0;
        let welcome = welcome.replace("\r\n", "\n");
        let artwork = artwork.replace("\r\n", "\n");
        if welcome.len() > 1024
            || artwork.len() > 2048
            || artwork.lines().count() > 16
            || artwork.lines().any(|line| line.chars().count() > 80)
            || [&welcome, &artwork]
                .into_iter()
                .any(|value| value.chars().any(|c| c.is_control() && c != '\n'))
        {
            return None;
        }
        Some(Self {
            revision,
            welcome,
            artwork,
            menu,
        })
    }

    pub fn menu(&self) -> &'static str {
        self.menu
    }
}

pub fn read(connection: &Connection) -> Result<Site, Error> {
    let row = connection
        .query_row(
            "SELECT revision, welcome, artwork, menu_order FROM settings_site WHERE id=1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()?;
    match row {
        Some((revision, welcome, artwork, menu)) => {
            Site::new(revision, &welcome, &artwork, &menu).ok_or(Error::Corrupt)
        }
        None => Ok(Site::default()),
    }
}

pub fn apply(transaction: &Transaction<'_>, mut proposed: Site) -> Result<Site, Error> {
    let current = read(transaction)?;
    if current.revision != proposed.revision {
        return Err(Error::Conflict);
    }
    if current == proposed {
        return Ok(current);
    }
    proposed.revision = current.revision.checked_add(1).ok_or(Error::Corrupt)?;
    transaction.execute(
        "INSERT INTO settings_site VALUES(1, ?1, ?2, ?3, ?4) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision, welcome=excluded.welcome, artwork=excluded.artwork, menu_order=excluded.menu_order",
        params![proposed.revision, proposed.welcome, proposed.artwork, proposed.menu],
    )?;
    Ok(proposed)
}
