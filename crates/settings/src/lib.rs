// SPDX-License-Identifier: AGPL-3.0-or-later
//! Validated node-owned browser palette and revisioned storage.
use bbsbox_storage::Migration;
use rusqlite::{Connection, OptionalExtension, Transaction, params};

pub fn migrations() -> [Migration; 1] {
    [Migration {
        module: "settings",
        version: 1,
        sql: include_str!("schema.sql"),
    }]
}

#[derive(Debug)]
pub enum Error {
    InvalidColor,
    LowContrast,
    Conflict,
    Corrupt,
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidColor => "expected a six-digit RGB color",
            Self::LowContrast => "theme colors do not meet the required contrast",
            Self::Conflict => "theme revision changed",
            Self::Corrupt => "stored theme is invalid",
            Self::Sqlite(_) => "theme storage operation failed",
        })
    }
}

impl std::error::Error for Error {}

impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Palette([String; 4]);

impl Default for Palette {
    fn default() -> Self {
        Self(["#000000", "#F5F5F5", "#00E5FF", "#FF4FD8"].map(String::from))
    }
}

impl Palette {
    pub fn new(colors: [&str; 4]) -> Result<Self, Error> {
        if colors.iter().any(|color| {
            color.len() != 7
                || !color.starts_with('#')
                || !color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
        }) {
            return Err(Error::InvalidColor);
        }
        let [background, text, accent, secondary] = colors.map(luminance);
        if [text, accent, secondary].iter().any(|foreground| {
            (background.max(*foreground) + 0.05) / (background.min(*foreground) + 0.05) < 4.5
        }) {
            return Err(Error::LowContrast);
        }
        Ok(Self(colors.map(str::to_ascii_uppercase)))
    }

    pub fn colors(&self) -> [&str; 4] {
        self.0.each_ref().map(String::as_str)
    }

    pub fn color_scheme(&self) -> &'static str {
        // At this luminance black and white give equal contrast against the background.
        if luminance(&self.0[0]) < 0.179 {
            "dark"
        } else {
            "light"
        }
    }
}

fn luminance(color: &str) -> f64 {
    let channel = |index| {
        let component =
            f64::from(u8::from_str_radix(&color[index..index + 2], 16).expect("validated RGB"))
                / 255.0;
        if component <= 0.04045 {
            component / 12.92
        } else {
            ((component + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5)
}

#[derive(Clone, Default)]
pub struct Theme {
    pub revision: i64,
    pub palette: Palette,
}

pub fn theme(connection: &Connection) -> Result<Theme, Error> {
    let row = connection.query_row(
        "SELECT revision, background, text_color, accent, secondary_accent FROM settings_theme WHERE id=1",
        [],
        |row| Ok((row.get::<_, i64>(0)?, [row.get::<_, String>(1)?, row.get(2)?, row.get(3)?, row.get(4)?])),
    ).optional()?;
    let Some((revision, colors)) = row else {
        return Ok(Theme::default());
    };
    let palette =
        Palette::new(colors.each_ref().map(String::as_str)).map_err(|_| Error::Corrupt)?;
    Ok(Theme { revision, palette })
}

/// Call from the same transaction that checks current SysOp authorization.
pub fn apply(
    transaction: &Transaction<'_>,
    expected: i64,
    palette: Palette,
) -> Result<Theme, Error> {
    let current = theme(transaction)?;
    if current.revision != expected {
        return Err(Error::Conflict);
    }
    if current.palette == palette {
        return Ok(current);
    }
    let revision = expected.checked_add(1).ok_or(Error::Corrupt)?;
    let [background, text, accent, secondary] = palette.colors();
    transaction.execute(
        "INSERT INTO settings_theme VALUES (1, ?1, ?2, ?3, ?4, ?5) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision, background=excluded.background, text_color=excluded.text_color, accent=excluded.accent, secondary_accent=excluded.secondary_accent",
        params![revision, background, text, accent, secondary],
    )?;
    Ok(Theme { revision, palette })
}
