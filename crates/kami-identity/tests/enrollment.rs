use kami_identity::{
    Custody, EnrollmentJournal, EnrollmentRecord, EnrollmentState, KeyPurpose, KeySlot, SecretStore,
};
use kami_key_provider::{KeyError, KeyProvider, ProfileKeyId};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
};
use zeroize::Zeroizing;

#[derive(Default)]
struct State {
    keys: BTreeMap<KeySlot, Zeroizing<Vec<u8>>>,
    faults: BTreeMap<(KeySlot, bool), VecDeque<KeyError>>,
    creates: usize,
}
#[derive(Clone, Default)]
struct Store(Arc<Mutex<State>>);
impl Store {
    fn fault(&self, slot: &KeySlot, write: bool, error: KeyError) {
        self.0
            .lock()
            .unwrap()
            .faults
            .entry((slot.clone(), write))
            .or_default()
            .push_back(error);
    }
    fn lose(&self, slot: &KeySlot) {
        self.0.lock().unwrap().keys.remove(slot);
    }
}
impl SecretStore for Store {
    fn read(&self, slot: &KeySlot) -> Result<Zeroizing<Vec<u8>>, KeyError> {
        let mut state = self.0.lock().unwrap();
        if let Some(error) = state
            .faults
            .get_mut(&(slot.clone(), false))
            .and_then(VecDeque::pop_front)
        {
            return Err(error);
        }
        state
            .keys
            .get(slot)
            .map(|v| Zeroizing::new(v.to_vec()))
            .ok_or(KeyError::Missing)
    }
    fn create(&self, slot: &KeySlot, secret: &[u8]) -> Result<(), KeyError> {
        let mut state = self.0.lock().unwrap();
        if let Some(error) = state
            .faults
            .get_mut(&(slot.clone(), true))
            .and_then(VecDeque::pop_front)
        {
            return Err(error);
        }
        if state.keys.contains_key(slot) {
            return Err(KeyError::Corrupt);
        }
        state.creates += 1;
        state
            .keys
            .insert(slot.clone(), Zeroizing::new(secret.to_vec()));
        Ok(())
    }
}
struct Journal {
    profile: ProfileKeyId,
    state: EnrollmentState,
    database: bool,
    fail_pending: bool,
    fail_commit: bool,
}
impl Journal {
    fn fresh() -> Self {
        Self {
            profile: ProfileKeyId::new("0123456789abcdef0123456789abcdef").unwrap(),
            state: EnrollmentState::Fresh,
            database: false,
            fail_pending: false,
            fail_commit: false,
        }
    }
}
impl EnrollmentJournal for Journal {
    fn profile(&self) -> &ProfileKeyId {
        &self.profile
    }
    fn state(&self) -> Result<EnrollmentState, KeyError> {
        Ok(self.state.clone())
    }
    fn database_exists(&self) -> Result<bool, KeyError> {
        Ok(self.database)
    }
    fn save_pending(&mut self, record: &EnrollmentRecord) -> Result<(), KeyError> {
        if self.fail_pending {
            return Err(KeyError::PermissionDenied);
        }
        self.state = EnrollmentState::Pending(record.clone());
        Ok(())
    }
    fn save_enrolled(&mut self, record: &EnrollmentRecord) -> Result<(), KeyError> {
        if self.fail_commit {
            return Err(KeyError::Unavailable);
        }
        self.state = EnrollmentState::Enrolled(record.clone());
        Ok(())
    }
}

#[test]
fn initialization_reopens_same_identity_without_replacing_either_key() {
    let store = Store::default();
    let custody = Custody::new(store.clone());
    let mut journal = Journal::fresh();
    let first = custody.initialize(&mut journal).unwrap();
    let db = custody.load_database_key(&journal.profile).unwrap();
    // Simulated durable serialization/restart of the public record.
    let record = EnrollmentRecord::from_parts(
        KeySlot::from_persistent_string(&first.identity_handle().to_persistent_string()).unwrap(),
        KeySlot::from_persistent_string(&first.database_handle().to_persistent_string()).unwrap(),
        *first.public_key(),
    )
    .unwrap();
    journal.state = EnrollmentState::Enrolled(record);
    let second = Custody::new(store.clone())
        .initialize(&mut journal)
        .unwrap();
    assert_eq!(first, second);
    assert_eq!(store.0.lock().unwrap().creates, 2);
    assert!(db.with_bytes(|a| custody
        .load_database_key(&journal.profile)
        .unwrap()
        .with_bytes(|b| a == b)));
    assert_ne!(first.identity_handle(), first.database_handle());
    assert!(
        store.read(first.identity_handle()).unwrap().as_slice()
            != store.read(first.database_handle()).unwrap().as_slice(),
        "identity and database keys must be independently generated"
    );
    assert!(!format!("{first:?}").contains(journal.profile.as_str()));
}

#[test]
fn each_enrolled_slot_failure_is_returned_without_writes() {
    for purpose in [KeyPurpose::Identity, KeyPurpose::Database] {
        for error in [
            KeyError::Locked,
            KeyError::Unavailable,
            KeyError::Missing,
            KeyError::Corrupt,
            KeyError::PermissionDenied,
            KeyError::Other,
        ] {
            let store = Store::default();
            let custody = Custody::new(store.clone());
            let mut journal = Journal::fresh();
            let record = custody.initialize(&mut journal).unwrap();
            let slot = KeySlot::new(journal.profile.clone(), purpose);
            store.fault(&slot, false, error);
            assert_eq!(custody.initialize(&mut journal), Err(error));
            assert_eq!(store.0.lock().unwrap().creates, 2);
            custody.open_existing(&record).unwrap();
        }
    }
}

#[test]
fn lost_identity_or_database_key_never_causes_replacement() {
    for purpose in [KeyPurpose::Identity, KeyPurpose::Database] {
        let store = Store::default();
        let custody = Custody::new(store.clone());
        let mut journal = Journal::fresh();
        let _record = custody.initialize(&mut journal).unwrap();
        store.lose(&KeySlot::new(journal.profile.clone(), purpose));
        assert_eq!(custody.initialize(&mut journal), Err(KeyError::Missing));
        assert_eq!(store.0.lock().unwrap().creates, 2);
    }
}

#[test]
fn replaced_identity_blocks_instead_of_adopting_the_new_key() {
    let store = Store::default();
    let custody = Custody::new(store.clone());
    let mut journal = Journal::fresh();
    let record = custody.initialize(&mut journal).unwrap();
    store.0.lock().unwrap().keys.insert(
        record.identity_handle().clone(),
        Zeroizing::new(vec![7; 32]),
    );
    assert_eq!(custody.initialize(&mut journal), Err(KeyError::Corrupt));
    assert_eq!(store.0.lock().unwrap().creates, 2);
}

#[test]
fn fresh_profile_with_existing_keys_or_database_refuses_enrollment() {
    let store = Store::default();
    let custody = Custody::new(store.clone());
    let mut journal = Journal::fresh();
    let _record = custody.initialize(&mut journal).unwrap();
    journal.state = EnrollmentState::Fresh;
    assert_eq!(custody.initialize(&mut journal), Err(KeyError::Corrupt));
    assert_eq!(store.0.lock().unwrap().creates, 2);
    let mut other = Journal::fresh();
    other.database = true;
    assert_eq!(
        Custody::new(Store::default()).initialize(&mut other),
        Err(KeyError::Corrupt)
    );
}

#[test]
fn first_read_failures_and_journal_failure_do_not_provision() {
    for error in [
        KeyError::Locked,
        KeyError::Unavailable,
        KeyError::PermissionDenied,
        KeyError::Corrupt,
        KeyError::Other,
    ] {
        let store = Store::default();
        let custody = Custody::new(store.clone());
        let mut journal = Journal::fresh();
        store.fault(
            &KeySlot::new(journal.profile.clone(), KeyPurpose::Identity),
            false,
            error,
        );
        assert_eq!(custody.initialize(&mut journal), Err(error));
        assert_eq!(store.0.lock().unwrap().creates, 0);
        assert_eq!(journal.state, EnrollmentState::Fresh);
    }
    let store = Store::default();
    let mut journal = Journal::fresh();
    journal.fail_pending = true;
    assert_eq!(
        Custody::new(store.clone()).initialize(&mut journal),
        Err(KeyError::PermissionDenied)
    );
    assert_eq!(store.0.lock().unwrap().creates, 0);
}

#[test]
fn pending_resume_only_completes_before_database_creation() {
    let store = Store::default();
    let custody = Custody::new(store.clone());
    let mut journal = Journal::fresh();
    let db_slot = KeySlot::new(journal.profile.clone(), KeyPurpose::Database);
    store.fault(&db_slot, true, KeyError::Locked);
    assert_eq!(custody.initialize(&mut journal), Err(KeyError::Locked));
    assert!(matches!(journal.state, EnrollmentState::Pending(_)));
    assert_eq!(store.0.lock().unwrap().creates, 1);
    journal.database = true;
    assert_eq!(custody.initialize(&mut journal), Err(KeyError::Corrupt));
    assert_eq!(store.0.lock().unwrap().creates, 1);
    journal.database = false;
    let record = custody.initialize(&mut journal).unwrap();
    custody.open_existing(&record).unwrap();
    assert_eq!(store.0.lock().unwrap().creates, 2);
}

#[test]
fn failure_before_identity_write_cannot_silently_generate_a_new_identity() {
    let store = Store::default();
    let mut journal = Journal::fresh();
    let slot = KeySlot::new(journal.profile.clone(), KeyPurpose::Identity);
    store.fault(&slot, true, KeyError::Unavailable);
    let custody = Custody::new(store.clone());
    assert_eq!(custody.initialize(&mut journal), Err(KeyError::Unavailable));
    assert_eq!(custody.initialize(&mut journal), Err(KeyError::Missing));
    assert_eq!(store.0.lock().unwrap().creates, 0);
}

#[test]
fn journal_commit_failure_reuses_both_provisioned_keys() {
    let store = Store::default();
    let mut journal = Journal::fresh();
    journal.fail_commit = true;
    let custody = Custody::new(store.clone());
    assert_eq!(custody.initialize(&mut journal), Err(KeyError::Unavailable));
    let pending = journal.state.clone();
    journal.fail_commit = false;
    let record = custody.initialize(&mut journal).unwrap();
    assert_eq!(pending, EnrollmentState::Pending(record));
    assert_eq!(store.0.lock().unwrap().creates, 2);
}

#[test]
fn malformed_keys_and_cross_profile_records_fail_closed() {
    let store = Store::default();
    let custody = Custody::new(store.clone());
    let mut journal = Journal::fresh();
    let record = custody.initialize(&mut journal).unwrap();
    for length in [0, 31, 33, 100] {
        store.0.lock().unwrap().keys.insert(
            record.database_handle().clone(),
            Zeroizing::new(vec![0; length]),
        );
        assert_eq!(
            custody.load_database_key(&journal.profile).unwrap_err(),
            KeyError::Corrupt
        );
    }
    journal.profile = ProfileKeyId::new("1123456789abcdef0123456789abcdef").unwrap();
    assert_eq!(custody.initialize(&mut journal), Err(KeyError::Corrupt));
    assert_eq!(
        EnrollmentRecord::from_parts(
            record.database_handle().clone(),
            record.identity_handle().clone(),
            *record.public_key()
        ),
        Err(KeyError::Corrupt)
    );
}

#[test]
fn key_loss_reenrollment_uses_new_namespace_and_new_public_identity() {
    let store = Store::default();
    let custody = Custody::new(store.clone());
    let mut old = Journal::fresh();
    let previous = custody.initialize(&mut old).unwrap();
    store.lose(previous.identity_handle());
    assert_eq!(custody.initialize(&mut old), Err(KeyError::Missing));
    // Explicit user-approved re-enrollment: distinct namespace, empty state.
    let mut new = Journal::fresh();
    new.profile = ProfileKeyId::new("2123456789abcdef0123456789abcdef").unwrap();
    let replacement = custody.initialize(&mut new).unwrap();
    assert_ne!(replacement.public_key(), previous.public_key());
    assert_ne!(replacement.identity_handle(), previous.identity_handle());
    // Old state is not reset, migrated, recovered or silently redirected.
    assert_eq!(custody.initialize(&mut old), Err(KeyError::Missing));
}
