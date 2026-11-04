use vodozemac::olm::{Account, OlmMessage, SessionConfig};

// Supplemental upstream smoke test, not a Kami skipped-key limit guarantee.
#[test]
fn upstream_dynamic_replay_and_small_reorder_smoke() {
    let mut bob = Account::new();
    bob.generate_one_time_keys(1);
    let k = *bob.one_time_keys().values().next().unwrap();
    let alice = Account::new();
    let mut a = alice
        .create_outbound_session(SessionConfig::version_1(), bob.curve25519_key(), k)
        .unwrap();
    let first = a.encrypt("one").unwrap();
    let p = match first.clone() {
        OlmMessage::PreKey(x) => x,
        _ => panic!(),
    };
    let mut b = bob
        .create_inbound_session(SessionConfig::version_1(), alice.curve25519_key(), &p)
        .unwrap()
        .session;
    assert!(b.decrypt(&first).is_err());
    let two = a.encrypt("two").unwrap();
    let three = a.encrypt("three").unwrap();
    assert_eq!(b.decrypt(&three).unwrap(), b"three");
    assert_eq!(b.decrypt(&two).unwrap(), b"two");
}
