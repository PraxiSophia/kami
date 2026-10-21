use kami_identity::{IdentityError, Invitation};
mod common;

#[test]
fn descriptor_roundtrips_without_trust_or_live_authentication() {
    let descriptor = common::invitation(11, 12);
    let bytes = descriptor.to_bytes();
    assert_eq!(Invitation::parse_bounded(&bytes).unwrap(), descriptor);
    for length in 0..bytes.len() {
        assert!(Invitation::parse_bounded(&bytes[..length]).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert_eq!(
        Invitation::parse_bounded(&trailing),
        Err(IdentityError::Malformed)
    );
    let mut weak = bytes.clone();
    weak[5..37].fill(0);
    assert_eq!(
        Invitation::parse_bounded(&weak),
        Err(IdentityError::InvalidKey)
    );
    let mut forgery = bytes;
    *forgery.last_mut().unwrap() ^= 1;
    assert_eq!(
        Invitation::parse_bounded(&forgery),
        Err(IdentityError::ForgedBinding)
    );
    assert_eq!(format!("{descriptor:?}"), "Invitation([REDACTED])");
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config { cases: 128, failure_persistence: None, ..Default::default() })]
    #[test]
    fn arbitrary_input_is_bounded_and_only_canonical_forms_roundtrip(bytes in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..1024)) {
        if let Ok(invitation) = Invitation::parse_bounded(&bytes) {
            proptest::prop_assert_eq!(invitation.to_bytes(), bytes.clone());
        }
        if let Ok(proof) = kami_identity::PeerBindingProof::parse_bounded(&bytes) {
            proptest::prop_assert_eq!(proof.to_bytes(), bytes);
        }
    }
}

#[test]
fn untrusted_descriptor_rejects_oversize_unsupported_and_trailing_data() {
    assert_eq!(
        Invitation::parse_bounded(&[0; 513]),
        Err(IdentityError::Oversized)
    );
    assert_eq!(
        Invitation::parse_bounded(b"KINV\x02"),
        Err(IdentityError::UnsupportedVersion)
    );
    assert!(Invitation::parse_bounded(b"KINV\x01").is_err());
    assert!(Invitation::parse_bounded(&[]).is_err());
}
