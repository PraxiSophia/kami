use kami_e2ee::state::{StateEnvelope, StateVersion};

#[test]
fn state_round_trip_and_rollback_rejection() {
    let s = StateEnvelope::new(2, vec![1, 2]).unwrap();
    let b = s.encode().unwrap();
    assert_eq!(StateEnvelope::decode(&b, 2).unwrap(), s);
    assert!(StateEnvelope::decode(&b, 3).is_err());
}

#[test]
fn future_generation_is_not_the_expected_snapshot() {
    let bytes = StateEnvelope::new(4, vec![1, 2]).unwrap().encode().unwrap();
    assert!(StateEnvelope::decode(&bytes, 3).is_err());
}

#[test]
fn unknown_version_and_trailing_bytes_rejected() {
    let mut b = StateEnvelope::new(1, vec![]).unwrap().encode().unwrap();
    b[12] = 9;
    assert!(matches!(
        StateEnvelope::decode(&b, 0),
        Err(kami_e2ee::Error::StateVersion { .. })
    ));
    let mut trailing = StateEnvelope::new(1, vec![]).unwrap().encode().unwrap();
    trailing.push(1);
    assert!(matches!(
        StateEnvelope::decode(&trailing, 1),
        Err(kami_e2ee::Error::StateEncoding)
    ));
    assert_eq!(StateVersion::V1.0, 1);
}
