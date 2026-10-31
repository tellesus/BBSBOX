// SPDX-License-Identifier: AGPL-3.0-or-later
//! Admission floors leave room for receipts/control operations; ENOSPC is still handled by SQLite.
use crate::{Error, Result};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

pub trait SpaceProbe: Send + Sync {
    fn available(&self, path: &Path) -> std::io::Result<u64>;
}
pub struct FilesystemSpace;
impl SpaceProbe for FilesystemSpace {
    fn available(&self, path: &Path) -> std::io::Result<u64> {
        fs2::available_space(path)
    }
}
#[derive(Clone, Copy)]
pub enum Class {
    Content,
    Control,
}
#[derive(Clone, Copy)]
pub struct Budget {
    pub content_floor: u64,
    pub control_floor: u64,
    pub max_blob_bytes: u64,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            content_floor: 256 * 1024 * 1024,
            control_floor: 32 * 1024 * 1024,
            max_blob_bytes: 64 * 1024 * 1024,
        }
    }
}
impl Budget {
    pub fn validate(self) -> Result<Self> {
        if self.content_floor < self.control_floor
            || self.max_blob_bytes == 0
            || self.max_blob_bytes > i64::MAX as u64
        {
            return Err(Error::InvalidConfiguration);
        }
        Ok(self)
    }
    pub(crate) fn admit(
        self,
        probe: &dyn SpaceProbe,
        path: &Path,
        class: Class,
        growth: u64,
    ) -> Result<()> {
        let floor = match class {
            Class::Content => self.content_floor,
            Class::Control => self.control_floor,
        };
        let needed = floor
            .checked_add(growth)
            .ok_or(Error::InvalidConfiguration)?;
        let available = probe.available(path)?;
        if available < needed {
            return Err(Error::DiskPressure { available, needed });
        }
        Ok(())
    }
}
pub(crate) struct Admission {
    pub budget: Budget,
    pub probe: Arc<dyn SpaceProbe>,
    pub path: PathBuf,
    pub class: Class,
}
impl Admission {
    pub fn check(&self, growth: u64) -> Result<()> {
        self.budget
            .admit(self.probe.as_ref(), &self.path, self.class, growth)
    }
}
