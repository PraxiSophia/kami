use kami_e2ee::state::StateEnvelope;
use kami_e2ee::types::ApplicationContext;
use kami_e2ee::{ConversationId, DeviceId, E2eeProvider, MessageId, VodozemacProvider};
use vodozemac::olm::{Account, SessionConfig};

fn ctx() -> ApplicationContext {
    ApplicationContext {
        conversation: ConversationId::from_u128(1),
        message: MessageId::from_u128(2),
        sender: DeviceId::from_bytes([3; 32]),
        recipient: DeviceId::from_bytes([4; 32]),
    }
}

fn provider() -> VodozemacProvider {
    let mut bob = Account::new();
    bob.generate_one_time_keys(1);
    let key = *bob.one_time_keys().values().next().unwrap();
    let alice = Account::new();
    let session = alice
        .create_outbound_session(SessionConfig::version_1(), bob.curve25519_key(), key)
        .unwrap();
    VodozemacProvider::new(session)
}

#[test]
fn provider_rejects_direct_unknown_state_version_before_context_gate() {
    let p = provider();
    let mut state = StateEnvelope::initial(vec![]).unwrap();
    state.version = kami_e2ee::StateVersion(9);
    let expected = Err(kami_e2ee::Error::StateVersion {
        found: 9,
        expected: 1,
    });
    assert_eq!(p.encrypt(&state, b"hello", &ctx()), expected);
    assert_eq!(
        p.decrypt(&state, b"{}", &ctx()),
        Err(kami_e2ee::Error::StateVersion {
            found: 9,
            expected: 1
        })
    );
}

#[test]
fn oversized_ciphertext_is_rejected_before_the_context_gate() {
    let p = provider();
    let state = StateEnvelope::initial(vec![]).unwrap();
    let ciphertext = vec![b' '; 65_537];
    assert_eq!(
        p.decrypt(&state, &ciphertext, &ctx()),
        Err(kami_e2ee::Error::Provider(
            kami_e2ee::ProviderError::Ciphertext
        ))
    );
}

#[test]
fn public_boundary_rejects_fork_and_future_snapshots() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/olm-v1.json")).unwrap();
    let before = fixture["sender_before"].as_str().unwrap();
    let session = vodozemac::olm::Session::from_pickle(serde_json::from_str(before).unwrap());
    let p = VodozemacProvider::new(session);
    let state = StateEnvelope::initial(before.as_bytes().to_vec()).unwrap();
    assert_eq!(
        p.encrypt(&state, b"hello", &ctx()),
        Err(kami_e2ee::Error::AadUnsupported)
    );
    let mut candidate = state.clone();
    candidate.generation = 1;
    assert_eq!(
        p.encrypt(&candidate, b"hello", &ctx()),
        Err(kami_e2ee::Error::StateRollback {
            found: 1,
            expected: 0
        })
    );
    candidate.generation = 0;
    candidate.provider = fixture["sender_after"]
        .as_str()
        .unwrap()
        .as_bytes()
        .to_vec();
    assert_eq!(
        p.encrypt(&candidate, b"hello", &ctx()),
        Err(kami_e2ee::Error::Provider(kami_e2ee::ProviderError::State))
    );
    assert_eq!(
        p.decrypt(&candidate, b"{}", &ctx()),
        Err(kami_e2ee::Error::Provider(kami_e2ee::ProviderError::State))
    );
    assert_eq!(
        p.encrypt(&state, b"hello", &ctx()),
        Err(kami_e2ee::Error::AadUnsupported)
    );
}

#[test]
fn valid_ciphertext_and_every_context_substitution_stay_fail_closed() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/olm-v1.json")).unwrap();
    let before = fixture["receiver_before"].as_str().unwrap();
    let session = vodozemac::olm::Session::from_pickle(serde_json::from_str(before).unwrap());
    let p = VodozemacProvider::new(session);
    let state = StateEnvelope::initial(before.as_bytes().to_vec()).unwrap();
    let ciphertext = fixture["ciphertext"].as_str().unwrap().as_bytes();
    for context in [
        ctx(),
        ApplicationContext {
            conversation: ConversationId::from_u128(9),
            ..ctx()
        },
        ApplicationContext {
            message: MessageId::from_u128(9),
            ..ctx()
        },
        ApplicationContext {
            sender: DeviceId::from_bytes([9; 32]),
            ..ctx()
        },
        ApplicationContext {
            recipient: DeviceId::from_bytes([9; 32]),
            ..ctx()
        },
    ] {
        assert_eq!(
            p.decrypt(&state, ciphertext, &context),
            Err(kami_e2ee::Error::AadUnsupported)
        );
        assert_eq!(
            p.encrypt(&state, b"hello", &context),
            Err(kami_e2ee::Error::AadUnsupported)
        );
    }
}

#[test]
fn native_aad_contract_is_separate_from_quarantined_envelope_composition() {
    fn assert_native_contract<P: kami_e2ee::NativeAadProvider>(provider: &P) {
        assert!(!provider.supports_aad());
        let state = StateEnvelope::initial(vec![]).unwrap();
        assert_eq!(
            provider.encrypt(&state, b"hello", &ctx()),
            Err(kami_e2ee::Error::AadUnsupported)
        );
        assert_eq!(
            provider.decrypt(&state, b"{}", &ctx()),
            Err(kami_e2ee::Error::AadUnsupported)
        );
    }
    // The compatibility name denotes exactly the same native-AAD contract.
    fn legacy<P: E2eeProvider>(provider: &P) {
        assert_native_contract(provider);
    }
    legacy(&provider());
}

#[test]
fn provider_contract_is_immutable_staged_and_aad_explicit() {
    let p = provider();
    let state = StateEnvelope::initial(vec![]).unwrap();
    assert_eq!(
        p.encrypt(&state, b"hello", &ctx()),
        Err(kami_e2ee::Error::AadUnsupported)
    );
    assert_eq!(
        p.decrypt(&state, b"{}", &ctx()),
        Err(kami_e2ee::Error::AadUnsupported)
    );
    assert!(!p.supports_aad());
    assert_eq!(state, StateEnvelope::initial(vec![]).unwrap());
}
