// SPDX-License-Identifier: AGPL-3.0-or-later
//! Convert an organizer's wall time to a fixed instant using offline IANA data.
use jiff::{
    civil::DateTime,
    tz::{AmbiguousOffset, Offset, TimeZone, TimeZoneDatabase},
};
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

static ZONES: LazyLock<TimeZoneDatabase> = LazyLock::new(TimeZoneDatabase::bundled);

#[derive(Clone, Copy)]
pub enum Occurrence {
    First,
    Second,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScheduleError {
    InvalidDate,
    UnknownZone,
    SkippedTime,
    RepeatedTime,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Schedule {
    local: String,
    zone: String,
    offset_seconds: i32,
    utc_millis: i64,
}

impl Schedule {
    pub fn resolve(
        local: &str,
        zone: &str,
        occurrence: Option<Occurrence>,
    ) -> Result<Self, ScheduleError> {
        let date = civil_time(local)?;
        if !zone_name(zone) {
            return Err(ScheduleError::UnknownZone);
        }
        let timezone = ZONES.get(zone).map_err(|_| ScheduleError::UnknownZone)?;
        let name = timezone
            .iana_name()
            .ok_or(ScheduleError::UnknownZone)?
            .to_owned();
        let ambiguous = timezone.to_ambiguous_zoned(date);
        let resolved = match (ambiguous.offset(), occurrence) {
            (AmbiguousOffset::Gap { .. }, _) => return Err(ScheduleError::SkippedTime),
            (AmbiguousOffset::Fold { .. }, None) => return Err(ScheduleError::RepeatedTime),
            (AmbiguousOffset::Fold { .. }, Some(Occurrence::First)) => ambiguous.earlier(),
            (AmbiguousOffset::Fold { .. }, Some(Occurrence::Second)) => ambiguous.later(),
            _ => ambiguous.unambiguous(),
        }
        .map_err(|_| ScheduleError::InvalidDate)?;
        Ok(Self {
            local: local.into(),
            zone: name,
            offset_seconds: resolved.offset().seconds(),
            utc_millis: resolved.timestamp().as_millisecond(),
        })
    }

    pub fn local(&self) -> &str {
        &self.local
    }
    pub fn zone(&self) -> &str {
        &self.zone
    }
    pub fn offset_seconds(&self) -> i32 {
        self.offset_seconds
    }
    pub fn utc_millis(&self) -> i64 {
        self.utc_millis
    }

    pub(crate) fn matches_current_rules(&self) -> bool {
        [None, Some(Occurrence::First), Some(Occurrence::Second)]
            .into_iter()
            .any(|occurrence| {
                Self::resolve(&self.local, &self.zone, occurrence).as_ref() == Ok(self)
            })
    }

    // Validate signed history with the recorded offset even if newer TZDB changes future rules.
    pub(crate) fn valid(&self) -> bool {
        if !zone_name(&self.zone) {
            return false;
        }
        let Ok(date) = civil_time(&self.local) else {
            return false;
        };
        let Ok(offset) = Offset::from_seconds(self.offset_seconds) else {
            return false;
        };
        date.to_zoned(TimeZone::fixed(offset))
            .is_ok_and(|date| date.timestamp().as_millisecond() == self.utc_millis)
    }
}

fn civil_time(value: &str) -> Result<DateTime, ScheduleError> {
    if value.len() != 16
        || !value.bytes().enumerate().all(|(index, byte)| match index {
            4 | 7 => byte == b'-',
            10 => byte == b'T',
            13 => byte == b':',
            _ => byte.is_ascii_digit(),
        })
        || value.starts_with("0000")
    {
        return Err(ScheduleError::InvalidDate);
    }
    value.parse().map_err(|_| ScheduleError::InvalidDate)
}

fn zone_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value.split('/').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'+' | b'_'))
        })
}
