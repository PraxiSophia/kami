use crate::{KeyPurpose, KeySlot};
use kami_key_provider::{KeyError, ProfileKeyId};
use std::fmt;

/// Public-only enrollment descriptor; never an E2EE or Peer-ID binding proof.
#[derive(Clone, Eq, PartialEq)]
pub struct EnrollmentRecord {
    identity: KeySlot,
    database: KeySlot,
    public_key: [u8; 32],
}
impl EnrollmentRecord {
    pub fn from_parts(
        identity: KeySlot,
        database: KeySlot,
        public_key: [u8; 32],
    ) -> Result<Self, KeyError> {
        if identity.purpose() != KeyPurpose::Identity
            || database.purpose() != KeyPurpose::Database
            || identity.profile() != database.profile()
        {
            return Err(KeyError::Corrupt);
        }
        let key =
            ed25519_dalek::VerifyingKey::from_bytes(&public_key).map_err(|_| KeyError::Corrupt)?;
        if key.is_weak() {
            return Err(KeyError::Corrupt);
        }
        Ok(Self {
            identity,
            database,
            public_key,
        })
    }
    pub fn identity_handle(&self) -> &KeySlot {
        &self.identity
    }
    pub fn database_handle(&self) -> &KeySlot {
        &self.database
    }
    pub fn public_key(&self) -> &[u8; 32] {
        &self.public_key
    }
    pub fn profile(&self) -> &ProfileKeyId {
        self.identity.profile()
    }
}
impl fmt::Debug for EnrollmentRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("EnrollmentRecord([REDACTED])")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EnrollmentState {
    Fresh,
    Pending(EnrollmentRecord),
    Enrolled(EnrollmentRecord),
}

/// Trusted service integration boundary, not a user-input DTO.
///
/// Implementers MUST hold the real OS-enforced exclusive profile lease for the
/// entire call. `state` and `database_exists` must reflect hardened durable local
/// state. Saves must be durable before returning (including directory metadata).
/// A new profile must have no old enrollment evidence or database. A Pending
/// record permits completion only before any database has been created.
/// The identity crate deliberately does not implement filesystem/SQL durability.
pub trait EnrollmentJournal {
    fn profile(&self) -> &ProfileKeyId;
    fn state(&self) -> Result<EnrollmentState, KeyError>;
    fn database_exists(&self) -> Result<bool, KeyError>;
    fn save_pending(&mut self, record: &EnrollmentRecord) -> Result<(), KeyError>;
    fn save_enrolled(&mut self, record: &EnrollmentRecord) -> Result<(), KeyError>;
}
