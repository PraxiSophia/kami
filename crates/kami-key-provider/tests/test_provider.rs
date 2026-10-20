#![cfg(feature = "test-provider")]
use kami_key_provider::{test_provider::TestKeyProvider, KeyError, KeyProvider, ProfileKeyId};

#[test]
fn fixture_survives_restart_and_failures_never_create_or_replace() {
    let profile = ProfileKeyId::new("0123456789abcdef0123456789abcdef").unwrap();
    let other = ProfileKeyId::new("1123456789abcdef0123456789abcdef").unwrap();
    let provider = TestKeyProvider::seeded(profile.clone(), [0x42; 32]);
    let restarted = provider.clone();
    assert_eq!(
        restarted
            .load_database_key(&profile)
            .unwrap()
            .with_bytes(|b| b[0]),
        0x42
    );
    for error in [
        KeyError::Locked,
        KeyError::Unavailable,
        KeyError::Missing,
        KeyError::Corrupt,
        KeyError::PermissionDenied,
        KeyError::Other,
    ] {
        provider.fail_next(&profile, error).unwrap();
        assert_eq!(restarted.load_database_key(&profile).unwrap_err(), error);
        assert_eq!(
            provider
                .load_database_key(&profile)
                .unwrap()
                .with_bytes(|b| b[0]),
            0x42
        );
    }
    assert_eq!(
        provider.load_database_key(&other).unwrap_err(),
        KeyError::Missing
    );
    assert_eq!(provider.calls(&profile).unwrap(), 13);
    assert_eq!(provider.calls(&other).unwrap(), 1);
    provider.lose_key(&profile).unwrap();
    assert_eq!(
        restarted.load_database_key(&profile).unwrap_err(),
        KeyError::Missing
    );
    assert!(!format!("{provider:?}").contains(profile.as_str()));
}

#[test]
fn concurrent_loads_share_counters_without_cross_slot_faults() {
    let profile = ProfileKeyId::new("0123456789abcdef0123456789abcdef").unwrap();
    let other = ProfileKeyId::new("1123456789abcdef0123456789abcdef").unwrap();
    let provider = TestKeyProvider::seeded(profile.clone(), [0x53; 32]);
    provider.fail_next(&other, KeyError::Locked).unwrap();
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let provider = provider.clone();
            let profile = profile.clone();
            std::thread::spawn(move || {
                for _ in 0..16 {
                    assert_eq!(
                        provider
                            .load_database_key(&profile)
                            .unwrap()
                            .with_bytes(|b| b[0]),
                        0x53
                    );
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(provider.calls(&profile).unwrap(), 128);
    assert_eq!(
        provider.load_database_key(&other).unwrap_err(),
        KeyError::Locked
    );
    assert_eq!(
        provider.load_database_key(&other).unwrap_err(),
        KeyError::Missing
    );
}
