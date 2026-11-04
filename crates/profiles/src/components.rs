// SPDX-License-Identifier: AGPL-3.0-or-later
//! Display preferences gate calls to installed, audience-aware data providers.
use super::Error;
use bbsbox_identity::LocalAccountId;
use rusqlite::{Row, Transaction};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub feed: bool,
    pub events: bool,
    pub games: bool,
    pub drops: bool,
}

impl Default for Selection {
    fn default() -> Self {
        Self {
            feed: false,
            events: false,
            games: false,
            drops: true,
        }
    }
}

impl Selection {
    pub(crate) fn read(row: &Row<'_>, offset: usize) -> rusqlite::Result<Self> {
        Ok(Self {
            feed: row.get(offset)?,
            events: row.get(offset + 1)?,
            games: row.get(offset + 2)?,
            drops: row.get(offset + 3)?,
        })
    }
}

#[derive(Clone, Copy)]
pub struct Viewer {
    pub account: LocalAccountId,
    pub profile: LocalAccountId,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Link {
    label: String,
    path: String,
}

impl Link {
    pub fn new(label: &str, path: &str) -> Option<Self> {
        if label.trim().is_empty()
            || label.len() > 240
            || label.chars().any(char::is_control)
            || path.len() > 512
            || !path.starts_with('/')
            || path.starts_with("//")
            || !path
                .bytes()
                .all(|byte| byte.is_ascii_graphic() && byte != b'\\')
        {
            return None;
        }
        Some(Self {
            label: label.into(),
            path: path.into(),
        })
    }

    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn path(&self) -> &str {
        &self.path
    }
}

/// The installed module checks its own source audience with both account IDs.
pub type Provider<T> = Arc<dyn Fn(&Transaction<'_>, Viewer) -> Result<T, Error> + Send + Sync>;

#[derive(Default)]
pub struct Providers {
    pub feed: Option<Provider<Option<Link>>>,
    pub events: Option<Provider<Vec<Link>>>,
    pub games: Option<Provider<Vec<Link>>>,
    pub drops: Option<Provider<Option<u64>>>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Display {
    pub feed: Option<Link>,
    pub events: Vec<Link>,
    pub games: Vec<Link>,
    pub drops: Option<u64>,
}

impl Providers {
    pub fn installed(&self) -> Selection {
        Selection {
            feed: self.feed.is_some(),
            events: self.events.is_some(),
            games: self.games.is_some(),
            drops: self.drops.is_some(),
        }
    }

    pub(crate) fn read(
        &self,
        tx: &Transaction<'_>,
        viewer: Viewer,
        public: bool,
        selection: Selection,
    ) -> Result<Display, Error> {
        let mut display = Display::default();
        if let Some(feed) = self.feed.as_ref().filter(|_| public && selection.feed) {
            display.feed = feed(tx, viewer)?;
        }
        if let Some(events) = self.events.as_ref().filter(|_| public && selection.events) {
            display.events = events(tx, viewer)?;
            display.events.truncate(25);
        }
        if let Some(games) = self.games.as_ref().filter(|_| public && selection.games) {
            display.games = games(tx, viewer)?;
            display.games.truncate(25);
        }
        if let Some(drops) = self.drops.as_ref().filter(|_| selection.drops) {
            display.drops = drops(tx, viewer)?;
        }
        Ok(display)
    }
}
