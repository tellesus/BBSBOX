// SPDX-License-Identifier: AGPL-3.0-or-later
//! Encrypted local identity checkpoints. Recovery kits remain a separate format and API.
use crate::{
    ActiveDevice, DeviceKey, RootKey,
    recovery::{Engine, Error, MAX_CLIENT_STATE},
    sealed::Sealer,
};
use bbsbox_credentials::{Credential, PersonId, Purposes};
use ed25519_dalek::SigningKey;
use zeroize::Zeroizing;

const DOMAIN: &str = "bbsbox.local-client/v1";
const MAX_PLAINTEXT: usize = MAX_CLIENT_STATE + 1024;

pub struct Session {
    root: RootKey,
    device: ActiveDevice,
    sealer: Sealer,
    requires_rejoin: bool,
}

pub struct Opened {
    pub session: Session,
    pub revision: u64,
    pub state: Zeroizing<Vec<u8>>,
}

impl Session {
    pub fn create(
        engine: &Engine,
        pass: Zeroizing<String>,
        purposes: Purposes,
    ) -> Result<Self, Error> {
        let root = RootKey::generate();
        Self::new(engine, root, pass, purposes, false)
    }

    fn new(
        engine: &Engine,
        root: RootKey,
        pass: Zeroizing<String>,
        purposes: Purposes,
        requires_rejoin: bool,
    ) -> Result<Self, Error> {
        let key = DeviceKey::generate();
        let credential = root.issue_device(key.public(), purposes)?;
        let device = key.activate(root.person_id(), credential)?;
        let sealer = Sealer::create(engine, DOMAIN, MAX_PLAINTEXT, pass)?;
        Ok(Self {
            root,
            device,
            sealer,
            requires_rejoin,
        })
    }

    pub fn recover(
        engine: &Engine,
        kit: &[u8],
        backup_pass: Zeroizing<String>,
        vault_pass: Zeroizing<String>,
        purposes: Purposes,
    ) -> Result<Opened, Error> {
        let restored = engine.import(kit, backup_pass)?;
        let (root, state) = restored.into_local_parts();
        Ok(Opened {
            session: Self::new(engine, root, vault_pass, purposes, true)?,
            revision: 1,
            state,
        })
    }

    pub fn open(engine: &Engine, encoded: &[u8], pass: Zeroizing<String>) -> Result<Opened, Error> {
        let (sealer, plaintext) = Sealer::open(engine, DOMAIN, 76, MAX_PLAINTEXT, encoded, pass)?;
        if plaintext[0] != 1 {
            return Err(Error::UnsupportedProfile);
        }
        let length = usize::from(u16::from_be_bytes(plaintext[65..67].try_into().unwrap()));
        let end = 67 + length;
        if length > bbsbox_credentials::MAX_CREDENTIAL_BYTES
            || plaintext.len() < end + 9
            || plaintext.len() - end - 9 > MAX_CLIENT_STATE
        {
            return Err(Error::InvalidKit);
        }
        let credential = Credential::verify(&plaintext[67..end])?;
        if credential.body().version() != 2 {
            return Err(Error::UnsupportedProfile);
        }
        let requires_rejoin = match plaintext[end] {
            0 => false,
            1 => true,
            _ => return Err(Error::InvalidKit),
        };
        let revision = u64::from_be_bytes(plaintext[end + 1..end + 9].try_into().unwrap());
        if revision == 0 || revision > i64::MAX as u64 {
            return Err(Error::InvalidKit);
        }
        let mut seed = Zeroizing::new([0; 32]);
        seed.copy_from_slice(&plaintext[1..33]);
        let root = RootKey(SigningKey::from_bytes(&seed), crate::fresh_incarnation());
        seed.copy_from_slice(&plaintext[33..65]);
        let device =
            DeviceKey(SigningKey::from_bytes(&seed)).activate(root.person_id(), credential)?;
        Ok(Opened {
            session: Self {
                root,
                device,
                sealer,
                requires_rejoin,
            },
            revision,
            state: Zeroizing::new(plaintext[end + 9..].to_vec()),
        })
    }

    /// The storage owner commits this complete encrypted record and its revision atomically.
    pub fn checkpoint(&self, revision: u64, state: &[u8]) -> Result<Vec<u8>, Error> {
        if revision == 0 || revision > i64::MAX as u64 || state.len() > MAX_CLIENT_STATE {
            return Err(Error::InvalidKit);
        }
        let root_seed = Zeroizing::new(self.root.0.to_bytes());
        let device_seed = Zeroizing::new(self.device.key.to_bytes());
        let credential = self.device.credential().encoded();
        let mut plaintext = Zeroizing::new(Vec::with_capacity(76 + credential.len() + state.len()));
        plaintext.push(1);
        plaintext.extend_from_slice(root_seed.as_slice());
        plaintext.extend_from_slice(device_seed.as_slice());
        plaintext.extend_from_slice(&(credential.len() as u16).to_be_bytes());
        plaintext.extend_from_slice(credential);
        plaintext.push(u8::from(self.requires_rejoin));
        plaintext.extend_from_slice(&revision.to_be_bytes());
        plaintext.extend_from_slice(state);
        self.sealer.seal(&plaintext)
    }

    pub fn person_id(&self) -> PersonId {
        self.root.person_id()
    }
    pub fn device(&self) -> &ActiveDevice {
        &self.device
    }
    pub fn control_incarnation(&self) -> &[u8; 16] {
        self.root.control_incarnation()
    }
    pub fn requires_rejoin(&self) -> bool {
        self.requires_rejoin
    }

    /// The group adapter supplies receive-side state explicitly. Local device secrets are excluded.
    pub fn export_recovery(
        &self,
        engine: &Engine,
        pass: Zeroizing<String>,
        receive_state: &[u8],
    ) -> Result<Vec<u8>, Error> {
        engine.export(&self.root, pass, receive_state)
    }
}
