use kami_key_provider::{KeyError, ProfileKeyId};
use std::fmt;
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum KeyPurpose {
    Identity,
    Database,
}

/// Durable opaque handle. Namespace and version are part of the representation.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct KeySlot {
    profile: ProfileKeyId,
    purpose: KeyPurpose,
}
impl KeySlot {
    pub fn new(profile: ProfileKeyId, purpose: KeyPurpose) -> Self {
        Self { profile, purpose }
    }
    pub fn profile(&self) -> &ProfileKeyId {
        &self.profile
    }
    pub fn purpose(&self) -> KeyPurpose {
        self.purpose
    }
    pub fn to_persistent_string(&self) -> String {
        format!(
            "kami:{}:v1:{}",
            match self.purpose {
                KeyPurpose::Identity => "identity",
                KeyPurpose::Database => "database",
            },
            self.profile.as_str()
        )
    }
    pub fn from_persistent_string(value: &str) -> Result<Self, KeyError> {
        let parts: Vec<_> = value.split(':').collect();
        if parts.len() != 4 || parts[0] != "kami" || parts[2] != "v1" {
            return Err(KeyError::Corrupt);
        }
        let purpose = match parts[1] {
            "identity" => KeyPurpose::Identity,
            "database" => KeyPurpose::Database,
            _ => return Err(KeyError::Corrupt),
        };
        Ok(Self::new(ProfileKeyId::new(parts[3])?, purpose))
    }
}
impl fmt::Debug for KeySlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("KeySlot([REDACTED])")
    }
}

/// Adapter boundary. Implementations must never replace an existing credential.
/// `create` is called only while the service holds its exclusive canonical
/// profile lease. Stores with upsert-only APIs require that cooperative lease;
/// unrelated credential-store writers remain outside the threat model.
/// Never expose these methods through CLI/IPC. Reads return zeroizing buffers.
pub trait SecretStore: Send + Sync {
    fn read(&self, slot: &KeySlot) -> Result<Zeroizing<Vec<u8>>, KeyError>;
    fn create(&self, slot: &KeySlot, secret: &[u8]) -> Result<(), KeyError>;
}

pub(crate) fn read_key(
    store: &impl SecretStore,
    slot: &KeySlot,
) -> Result<Zeroizing<[u8; 32]>, KeyError> {
    let bytes = store.read(slot)?;
    if bytes.len() != 32 {
        return Err(KeyError::Corrupt);
    }
    let mut key = Zeroizing::new([0; 32]);
    key.copy_from_slice(&bytes);
    Ok(key)
}
