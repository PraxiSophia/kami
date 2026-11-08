use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use zeroize::Zeroizing;
struct Store {
    row: Vec<u8>,
    fault: Option<(KeyPurpose, KeyError)>,
    seed: Vec<u8>,
    anchor: Option<Vec<u8>>,
    creates: AtomicUsize,
}
impl SecretStore for Store {
    fn read(&self, slot: &KeySlot) -> Result<Zeroizing<Vec<u8>>, KeyError> {
        if let Some((purpose, error)) = self.fault {
            if purpose == slot.purpose() {
                return Err(error);
            }
        }
        Ok(Zeroizing::new(if slot.purpose() == KeyPurpose::Anchor {
            self.anchor
                .clone()
                .unwrap_or_else(|| digest(&self.row).to_vec())
        } else if slot.purpose() == KeyPurpose::Identity {
            self.seed.clone()
        } else {
            vec![9; 32]
        }))
    }
    fn create(&self, _: &KeySlot, _: &[u8]) -> Result<(), KeyError> {
        self.creates.fetch_add(1, Ordering::SeqCst);
        Err(KeyError::Other)
    }
}
fn fixture() -> (ProfileKeyId, Store) {
    let profile = ProfileKeyId::new("0123456789abcdef0123456789abcdef").unwrap();
    let public = ed25519_dalek::SigningKey::from_bytes(&[7; 32])
        .verifying_key()
        .to_bytes();
    let mut row = vec![1, 1];
    row.extend_from_slice(profile.as_str().as_bytes());
    row.extend_from_slice(&public);
    (
        profile,
        Store {
            row,
            seed: vec![7; 32],
            anchor: None,
            fault: None,
            creates: AtomicUsize::new(0),
        },
    )
}
#[test]
fn load_faults_mismatches_and_retries_never_create_or_recover_keys() {
    for purpose in [
        KeyPurpose::Identity,
        KeyPurpose::Database,
        KeyPurpose::Anchor,
    ] {
        for error in [
            KeyError::Locked,
            KeyError::Missing,
            KeyError::Corrupt,
            KeyError::PermissionDenied,
            KeyError::Unavailable,
            KeyError::Other,
        ] {
            let (profile, mut store) = fixture();
            store.fault = Some((purpose, error));
            for _ in 0..2 {
                assert_eq!(verify_with_store(&store, &profile, &store.row), Err(error));
            }
            assert_eq!(store.creates.load(Ordering::SeqCst), 0);
        }
    }
    for seed in [vec![], vec![0; 31], vec![0; 32], vec![0; 33]] {
        let (profile, mut store) = fixture();
        store.seed = seed;
        assert_eq!(
            verify_with_store(&store, &profile, &store.row),
            Err(KeyError::Corrupt)
        );
        assert_eq!(store.creates.load(Ordering::SeqCst), 0);
    }
    for anchor in [vec![], vec![0; 31], vec![0; 32], vec![0; 33]] {
        let (profile, mut store) = fixture();
        store.anchor = Some(anchor);
        assert_eq!(
            verify_with_store(&store, &profile, &store.row),
            Err(KeyError::Corrupt)
        );
        assert_eq!(store.creates.load(Ordering::SeqCst), 0);
    }
    let (profile, store) = fixture();
    for _ in 0..2 {
        assert_eq!(verify_with_store(&store, &profile, &store.row), Ok(()));
    }
    assert_eq!(store.creates.load(Ordering::SeqCst), 0);
}
