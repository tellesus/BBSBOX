// SPDX-License-Identifier: AGPL-3.0-or-later
use bbsbox_client_identity::recovery::{Engine, MAX_CLIENT_STATE};
use bbsbox_client_vault::Vault;
use bbsbox_credentials::{Purpose, Purposes};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
    sync::Mutex,
};
use zeroize::Zeroizing;

const LOCAL_STATE: &[u8] = b"native durable state and queued ciphertext";
const RECEIVE_STATE: &[u8] = b"native receive-side recovery state";

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    Create,
    Unlock,
    Recover,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fixture {
    pub action: Action,
    pub vault: PathBuf,
    pub passphrase_file: PathBuf,
    pub backup: Option<PathBuf>,
    pub backup_passphrase_file: Option<PathBuf>,
    pub backup_output: Option<PathBuf>,
}

#[derive(Serialize)]
pub struct Status {
    pub person: Option<String>,
    pub device: Option<String>,
    pub requires_rejoin: Option<bool>,
    pub revision: Option<u64>,
    pub checking: bool,
}
impl Status {
    fn of(vault: Option<&Vault>, checking: bool) -> Self {
        Self {
            person: vault.map(|vault| vault.person_id().to_string()),
            device: vault.map(|vault| vault.credential().id().to_string()),
            requires_rejoin: vault.map(Vault::requires_rejoin),
            revision: vault.map(Vault::revision),
            checking,
        }
    }
}

pub struct Backend {
    path: PathBuf,
    engine: Engine,
    identity: Mutex<Option<Vault>>,
}

fn purposes() -> Purposes {
    Purposes::new(&[Purpose::Authorship, Purpose::IdentityProof]).expect("client purposes")
}

pub fn read_bounded(path: &Path, maximum: usize) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|error| error.to_string())?
        .take((maximum + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > maximum {
        return Err("Client input exceeds its limit".into());
    }
    Ok(bytes)
}

fn passphrase(path: &Path) -> Result<Zeroizing<String>, String> {
    let mut bytes = Zeroizing::new(Vec::new());
    File::open(path)
        .map_err(|error| error.to_string())?
        .take(1025)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > 1024 {
        return Err("Passphrase input exceeds its limit".into());
    }
    Ok(Zeroizing::new(
        std::str::from_utf8(&bytes)
            .map_err(|error| error.to_string())?
            .to_owned(),
    ))
}

impl Backend {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            engine: Engine::default(),
            identity: Mutex::new(None),
        }
    }
    pub fn status(&self, checking: bool) -> Result<Status, String> {
        let active = self
            .identity
            .try_lock()
            .map_err(|_| "Client operation is busy")?;
        Ok(Status::of(active.as_ref(), checking))
    }

    pub fn open(
        &self,
        pass: Zeroizing<String>,
        create: bool,
        checking: bool,
    ) -> Result<Status, String> {
        let mut active = self
            .identity
            .try_lock()
            .map_err(|_| "Client operation is busy")?;
        if active.is_some() {
            return Err("Identity is already unlocked".into());
        }
        let vault = if create {
            Vault::create(&self.path, &self.engine, pass, purposes())
        } else {
            Vault::unlock(&self.path, &self.engine, pass)
        }
        .map_err(|error| error.to_string())?;
        *active = Some(vault);
        Ok(Status::of(active.as_ref(), checking))
    }

    pub fn check(&self, fixture: &Fixture) -> Result<Status, String> {
        let mut active = self
            .identity
            .try_lock()
            .map_err(|_| "Client operation is busy")?;
        if active.is_some() {
            return Err("Identity is already unlocked".into());
        }
        let pass = passphrase(&fixture.passphrase_file)?;
        let mut vault = match fixture.action {
            Action::Create => Vault::create(&self.path, &self.engine, pass, purposes()),
            Action::Unlock => Vault::unlock(&self.path, &self.engine, pass),
            Action::Recover => {
                let kit = read_bounded(
                    fixture.backup.as_deref().ok_or("Missing backup")?,
                    MAX_CLIENT_STATE + 256,
                )?;
                let backup_pass = passphrase(
                    fixture
                        .backup_passphrase_file
                        .as_deref()
                        .ok_or("Missing backup passphrase")?,
                )?;
                Vault::recover(
                    &self.path,
                    &self.engine,
                    &kit,
                    backup_pass,
                    pass,
                    purposes(),
                )
            }
        }
        .map_err(|error| error.to_string())?;
        if matches!(fixture.action, Action::Create) {
            vault
                .checkpoint(1, Zeroizing::new(LOCAL_STATE.to_vec()))
                .map_err(|error| error.to_string())?;
        }
        let expected = if vault.requires_rejoin() {
            RECEIVE_STATE
        } else {
            LOCAL_STATE
        };
        if vault.state() != expected {
            return Err("Stored client state differs from the fixture".into());
        }
        if let Some(path) = &fixture.backup_output {
            let backup_pass = passphrase(
                fixture
                    .backup_passphrase_file
                    .as_deref()
                    .ok_or("Missing backup passphrase")?,
            )?;
            let kit = vault
                .export_recovery(&self.engine, backup_pass, RECEIVE_STATE)
                .map_err(|error| error.to_string())?;
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|error| error.to_string())?;
            use std::io::Write;
            file.write_all(&kit).map_err(|error| error.to_string())?;
            file.sync_all().map_err(|error| error.to_string())?;
        }
        *active = Some(vault);
        Ok(Status::of(active.as_ref(), true))
    }

    pub fn evidence(&self) -> Result<serde_json::Value, String> {
        let active = self
            .identity
            .try_lock()
            .map_err(|_| "Client operation is busy")?;
        let vault = active.as_ref().ok_or("Identity is locked")?;
        Ok(
            serde_json::json!({"person":vault.person_id().to_string(),"device":vault.credential().id().to_string(),
            "revision":vault.revision(),"requires_rejoin":vault.requires_rejoin(),
            "control_incarnation":vault.control_incarnation(),"state_verified":true}),
        )
    }
}
