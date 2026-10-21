use crate::{
    store::read_key, EnrollmentJournal, EnrollmentRecord, EnrollmentState, KeyPurpose, KeySlot,
    SecretStore,
};
use kami_key_provider::{DatabaseKey, KeyError, KeyProvider, ProfileKeyId};
use std::fmt;
use zeroize::Zeroizing;

/// Synchronous custody service. Inject a native store at composition time.
pub struct Custody<S>(S);
impl<S: SecretStore> Custody<S> {
    pub fn new(store: S) -> Self {
        Self(store)
    }

    /// Explicit enrollment or idempotent reopen under the caller's held lease.
    /// Missing enrolled keys NEVER cause generation. Re-enrollment requires a
    /// fresh random profile namespace, empty database, and contact reverification;
    /// there is intentionally no reset/delete/overwrite API.
    pub fn initialize(
        &self,
        journal: &mut impl EnrollmentJournal,
    ) -> Result<EnrollmentRecord, KeyError> {
        match journal.state()? {
            EnrollmentState::Fresh => {
                if journal.database_exists()? {
                    return Err(KeyError::Corrupt);
                }
                let identity = KeySlot::new(journal.profile().clone(), KeyPurpose::Identity);
                let database = KeySlot::new(journal.profile().clone(), KeyPurpose::Database);
                self.require_absent(&identity)?;
                self.require_absent(&database)?;
                let seed = random_key()?;
                let database_key = random_key()?;
                let record = EnrollmentRecord::from_parts(identity, database, public_key(&seed))?;
                journal.save_pending(&record)?;
                self.create_verified(record.identity_handle(), &seed)?;
                self.create_verified(record.database_handle(), &database_key)?;
                journal.save_enrolled(&record)?;
                Ok(record)
            }
            EnrollmentState::Pending(record) => {
                self.check_profile(journal.profile(), &record)?;
                if journal.database_exists()? {
                    return Err(KeyError::Corrupt);
                }
                self.check_identity(&record)?;
                match read_key(&self.0, record.database_handle()) {
                    Ok(_) => {}
                    Err(KeyError::Missing) => {
                        self.create_verified(record.database_handle(), &*random_key()?)?
                    }
                    Err(error) => return Err(error),
                }
                journal.save_enrolled(&record)?;
                Ok(record)
            }
            EnrollmentState::Enrolled(record) => {
                self.check_profile(journal.profile(), &record)?;
                self.open_existing(&record)?;
                Ok(record)
            }
        }
    }

    /// Sign only the fixed v1 Peer-ID binding transcript. Run on a bounded
    /// blocking custody worker, never inside a storage transaction/network task.
    pub fn create_peer_binding(
        &self,
        record: &EnrollmentRecord,
        public: &libp2p_identity::PublicKey,
    ) -> Result<crate::PeerBindingProof, crate::IdentityError> {
        let seed =
            read_key(&self.0, record.identity_handle()).map_err(crate::IdentityError::Custody)?;
        let key = ed25519_dalek::SigningKey::from_bytes(&seed);
        if key.verifying_key().to_bytes() != *record.public_key() {
            return Err(crate::IdentityError::Custody(KeyError::Corrupt));
        }
        crate::PeerBindingProof::sign(public, &key)
    }

    /// Ordinary startup: read-only, no provisioning, replacement or downgrade.
    pub fn open_existing(&self, record: &EnrollmentRecord) -> Result<(), KeyError> {
        self.check_identity(record)?;
        read_key(&self.0, record.database_handle())?;
        Ok(())
    }
    fn check_profile(
        &self,
        profile: &ProfileKeyId,
        record: &EnrollmentRecord,
    ) -> Result<(), KeyError> {
        if profile != record.profile() {
            return Err(KeyError::Corrupt);
        }
        Ok(())
    }
    fn check_identity(&self, record: &EnrollmentRecord) -> Result<(), KeyError> {
        let seed = read_key(&self.0, record.identity_handle())?;
        if public_key(&seed) != *record.public_key() {
            return Err(KeyError::Corrupt);
        }
        Ok(())
    }
    fn require_absent(&self, slot: &KeySlot) -> Result<(), KeyError> {
        match self.0.read(slot) {
            Err(KeyError::Missing) => Ok(()),
            Ok(_) => Err(KeyError::Corrupt),
            Err(error) => Err(error),
        }
    }
    fn create_verified(&self, slot: &KeySlot, bytes: &[u8; 32]) -> Result<(), KeyError> {
        self.0.create(slot, bytes)?;
        let readback = read_key(&self.0, slot)?;
        // Local readback equality is not an authentication protocol.
        if *readback != *bytes {
            return Err(KeyError::Corrupt);
        }
        Ok(())
    }
}
impl<S: SecretStore> KeyProvider for Custody<S> {
    fn load_database_key(&self, profile: &ProfileKeyId) -> Result<DatabaseKey, KeyError> {
        Ok(DatabaseKey::from_bytes(read_key(
            &self.0,
            &KeySlot::new(profile.clone(), KeyPurpose::Database),
        )?))
    }
}
impl<S> fmt::Debug for Custody<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Custody([REDACTED])")
    }
}
fn random_key() -> Result<Zeroizing<[u8; 32]>, KeyError> {
    let mut bytes = Zeroizing::new([0; 32]);
    getrandom::fill(bytes.as_mut()).map_err(|_| KeyError::Unavailable)?;
    Ok(bytes)
}
fn public_key(seed: &[u8; 32]) -> [u8; 32] {
    ed25519_dalek::SigningKey::from_bytes(seed)
        .verifying_key()
        .to_bytes()
}
