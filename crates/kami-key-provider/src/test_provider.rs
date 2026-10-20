//! Explicitly seeded, in-memory test fixture; never a production fallback.
use crate::{DatabaseKey, KeyError, KeyProvider, ProfileKeyId};
use std::{
    collections::{BTreeMap, VecDeque},
    fmt,
    sync::{Arc, Mutex},
};
use zeroize::Zeroizing;

#[derive(Default)]
struct State {
    keys: BTreeMap<ProfileKeyId, Zeroizing<[u8; 32]>>,
    faults: BTreeMap<ProfileKeyId, VecDeque<KeyError>>,
    calls: BTreeMap<ProfileKeyId, usize>,
}
/// Clones simulate reopening the same store, not cloning a database key.
#[derive(Clone)]
pub struct TestKeyProvider(Arc<Mutex<State>>);
impl TestKeyProvider {
    pub fn seeded(profile: ProfileKeyId, bytes: [u8; 32]) -> Self {
        let mut state = State::default();
        state.keys.insert(profile, Zeroizing::new(bytes));
        Self(Arc::new(Mutex::new(state)))
    }
    pub fn fail_next(&self, profile: &ProfileKeyId, error: KeyError) -> Result<(), KeyError> {
        self.0
            .lock()
            .map_err(|_| KeyError::Other)?
            .faults
            .entry(profile.clone())
            .or_default()
            .push_back(error);
        Ok(())
    }
    pub fn calls(&self, profile: &ProfileKeyId) -> Result<usize, KeyError> {
        Ok(*self
            .0
            .lock()
            .map_err(|_| KeyError::Other)?
            .calls
            .get(profile)
            .unwrap_or(&0))
    }
    pub fn lose_key(&self, profile: &ProfileKeyId) -> Result<(), KeyError> {
        self.0
            .lock()
            .map_err(|_| KeyError::Other)?
            .keys
            .remove(profile);
        Ok(())
    }
}
impl KeyProvider for TestKeyProvider {
    fn load_database_key(&self, profile: &ProfileKeyId) -> Result<DatabaseKey, KeyError> {
        let mut state = self.0.lock().map_err(|_| KeyError::Other)?;
        *state.calls.entry(profile.clone()).or_default() += 1;
        if let Some(error) = state.faults.get_mut(profile).and_then(VecDeque::pop_front) {
            return Err(error);
        }
        let key = state.keys.get(profile).ok_or(KeyError::Missing)?;
        Ok(DatabaseKey::from_bytes(Zeroizing::new(**key)))
    }
}
impl fmt::Debug for TestKeyProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TestKeyProvider([REDACTED])")
    }
}
