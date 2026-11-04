use kami_e2ee::types::ApplicationContext;
use kami_e2ee::{ConversationId, DeviceId, MessageId};

fn c() -> ApplicationContext {
    ApplicationContext {
        conversation: ConversationId::from_u128(1),
        message: MessageId::from_u128(2),
        sender: DeviceId::from_bytes([3; 32]),
        recipient: DeviceId::from_bytes([4; 32]),
    }
}

#[test]
fn context_encoding_is_deterministic_and_profile_bounded() {
    let p = kami_e2ee::Profile::v1(c());
    assert_eq!(p.encode().unwrap(), p.encode().unwrap());
    assert!(p.encode().unwrap().len() <= kami_e2ee::profile::MAX_PROFILE_BYTES);
}

#[test]
fn context_and_profile_match_fixed_layout() {
    let expected_context = [
        &[0, 0, 0, 16][..],
        &[0; 15],
        &[1],
        &[0, 0, 0, 16],
        &[0; 15],
        &[2],
        &[0, 0, 0, 32],
        &[3; 32],
        &[0, 0, 0, 32],
        &[4; 32],
    ]
    .concat();
    assert_eq!(c().encode().unwrap(), expected_context);
    let expected_profile = [b"KAMI-E2EE\x00\x01\x00\x70".as_slice(), &expected_context].concat();
    assert_eq!(
        kami_e2ee::Profile::v1(c()).encode().unwrap(),
        expected_profile
    );
}

#[test]
fn invalid_context_fails_before_encoding() {
    let invalid = ApplicationContext {
        conversation: ConversationId::from_u128(0),
        ..c()
    };
    assert!(invalid.encode().is_err());
}

#[test]
fn unsupported_profile_version_fails_closed() {
    let mut profile = kami_e2ee::Profile::v1(c());
    profile.version = kami_e2ee::ProfileVersion(2);
    assert!(profile.encode().is_err());
}
