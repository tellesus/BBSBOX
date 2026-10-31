// SPDX-License-Identifier: AGPL-3.0-or-later
//! Calendar scheduling is local wall time; claim expiry uses process-monotonic time.
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub trait Clock: Send + Sync {
    fn wall_millis(&self) -> i64;
    fn monotonic_millis(&self) -> i64;
}
pub struct SystemClock {
    start: Instant,
}
impl Default for SystemClock {
    fn default() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}
impl Clock for SystemClock {
    fn wall_millis(&self) -> i64 {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(value) => value.as_millis().min(i64::MAX as u128) as i64,
            Err(error) => -(error.duration().as_millis().min(i64::MAX as u128) as i64),
        }
    }
    fn monotonic_millis(&self) -> i64 {
        self.start.elapsed().as_millis().min(i64::MAX as u128) as i64
    }
}
