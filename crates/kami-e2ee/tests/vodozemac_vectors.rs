use vodozemac::olm::{Account, OlmMessage, SessionConfig};

#[test]
fn vodozemac_prekey_dynamic_smoke_round_trip() {
    let mut bob = Account::new();
    bob.generate_one_time_keys(1);
    let k = *bob.one_time_keys().values().next().unwrap();
    let alice = Account::new();
    let mut a = alice
        .create_outbound_session(SessionConfig::version_1(), bob.curve25519_key(), k)
        .unwrap();
    let m = a.encrypt("vector").unwrap();
    let p = match m.clone() {
        OlmMessage::PreKey(x) => x,
        _ => panic!("expected prekey"),
    };
    let r = bob
        .create_inbound_session(SessionConfig::version_1(), alice.curve25519_key(), &p)
        .unwrap();
    assert_eq!(r.plaintext, b"vector");
    assert_eq!(a.session_id(), r.session.session_id());
}
