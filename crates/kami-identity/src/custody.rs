use crate::{
    store::read_key, EnrollmentJournal, EnrollmentRecord, EnrollmentState, KeyPurpose, KeySlot,
    SecretStore,
};
use kami_key_provider::{DatabaseKey, KeyError, KeyProvider, ProfileKeyId};
use std::fmt;
use zeroize::Zeroizing;

/// Synchronous custody service. Inject a native store at composition time.
pub struct Custody<S>(pub(crate) S);
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

    /// Sign only the canonical v1 Olm account/prekey binding, using the enrolled
    /// device key. The separate Peer-ID proof must belong to that same device.
    /// Run on a bounded blocking custody worker, outside storage transactions.
    pub fn create_olm_binding(
        &self,
        record: &EnrollmentRecord,
        peer_binding: &crate::PeerBindingProof,
        data: crate::OlmBindingData,
    ) -> Result<crate::OlmBindingProof, crate::IdentityError> {
        peer_binding.verify_signature(*record.public_key())?;
        if peer_binding.peer_id() != data.publisher_peer {
            return Err(crate::IdentityError::WrongPeer);
        }
        let seed =
            read_key(&self.0, record.identity_handle()).map_err(crate::IdentityError::Custody)?;
        let key = ed25519_dalek::SigningKey::from_bytes(&seed);
        if key.verifying_key().to_bytes() != *record.public_key() {
            return Err(crate::IdentityError::Custody(KeyError::Corrupt));
        }
        crate::OlmBindingProof::sign(data, &key)
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
impl crate::NativeCustody {
    /// Purpose-limited existing-proof signing after fresh native anchor/key checks.
    /// No generic message signing or secret material leaves identity custody.
    pub fn create_olm_binding(
        &self,
        enrollment: &crate::VerifiedEnrollment,
        fresh_row: &[u8],
        peer_binding: &crate::PeerBindingProof,
        data: crate::OlmBindingData,
    ) -> Result<crate::OlmBindingProof, crate::IdentityError> {
        if fresh_row != enrollment.row {
            return Err(crate::IdentityError::Custody(KeyError::Corrupt));
        }
        let mut proofs = self.create_olm_bindings(enrollment, fresh_row, peer_binding, &[data])?;
        proofs.pop().ok_or(crate::IdentityError::Malformed)
    }
    /// Bounded batch of independent existing per-selection signatures, NOT an
    /// aggregate signature. One native custody lease/read avoids repeated OS I/O.
    pub fn create_olm_bindings(
        &self,
        enrollment: &crate::VerifiedEnrollment,
        fresh_row: &[u8],
        peer_binding: &crate::PeerBindingProof,
        data: &[crate::OlmBindingData],
    ) -> Result<Vec<crate::OlmBindingProof>, crate::IdentityError> {
        if data.is_empty() || data.len() > 52 || fresh_row != enrollment.row {
            return Err(crate::IdentityError::Malformed);
        }
        crate::verify_enrollment_for_transport(self, &enrollment.profile, fresh_row)
            .map_err(crate::IdentityError::Custody)?;
        let device: [u8; 32] = fresh_row[34..]
            .try_into()
            .map_err(|_| crate::IdentityError::Malformed)?;
        peer_binding.verify_signature(device)?;
        let public = libp2p_identity::ed25519::PublicKey::try_from_bytes(&device)
            .map_err(|_| crate::IdentityError::Malformed)?;
        let peer = libp2p_identity::PublicKey::from(public).to_peer_id();
        if peer_binding.peer_id() != peer || data.iter().any(|p| p.publisher_peer != peer) {
            return Err(crate::IdentityError::WrongPeer);
        }
        let seed = read_key(
            &self.custody.0,
            &KeySlot::new(enrollment.profile.clone(), KeyPurpose::Identity),
        )
        .map_err(crate::IdentityError::Custody)?;
        let key = ed25519_dalek::SigningKey::from_bytes(&seed);
        if key.verifying_key().to_bytes() != device {
            return Err(crate::IdentityError::Custody(KeyError::Corrupt));
        }
        let proofs = data
            .iter()
            .map(|data| crate::OlmBindingProof::sign(data.clone(), &key))
            .collect::<Result<Vec<_>, _>>()?;
        crate::verify_enrollment_for_transport(self, &enrollment.profile, fresh_row)
            .map_err(crate::IdentityError::Custody)?;
        Ok(proofs)
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
