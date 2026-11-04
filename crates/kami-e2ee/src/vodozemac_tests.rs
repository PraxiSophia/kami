use super::*;
use vodozemac::olm::{Account, SessionConfig};

#[derive(serde::Deserialize)]
struct Fixture {
    session_id: String,
    plaintext: String,
    ciphertext: String,
    sender_before: String,
    sender_after: String,
    receiver_before: String,
    receiver_after: String,
    state_bytes: Vec<u8>,
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!("../tests/fixtures/olm-v1.json")).unwrap()
}

fn anchored(bytes: &str, generation: u64) -> VodozemacProvider {
    let session = Session::from_pickle(serde_json::from_str(bytes).unwrap());
    let mut provider = VodozemacProvider::new(session);
    // Test-only simulation: there is deliberately no public commit/import API.
    provider.generation = generation;
    provider
}

#[test]
fn fixed_provider_encrypt_and_state_fixture() {
    let f = fixture();
    let provider = anchored(&f.sender_before, 7);
    let state = StateEnvelope::decode(&f.state_bytes, 7).unwrap();
    assert_eq!(state.provider, f.sender_before.as_bytes());
    assert_eq!(state.encode().unwrap(), f.state_bytes);
    assert_eq!(provider.session.session_id(), f.session_id);
    let result = provider
        .encrypt_unbound(&state, f.plaintext.as_bytes(), vec![])
        .unwrap();
    assert_eq!(result.ciphertext, f.ciphertext.as_bytes());
    assert_eq!(
        result.next_state,
        StateEnvelope::new(8, f.sender_after.as_bytes().to_vec()).unwrap()
    );
    assert_eq!(
        VodozemacProvider::encode_session(&provider.session).unwrap(),
        state.provider
    );
    assert!(provider.session_for(&result.next_state).is_err());
    let committed = anchored(&f.sender_after, 8);
    assert!(committed.session_for(&result.next_state).is_ok());
    assert!(committed.session_for(&state).is_err());
}

#[test]
fn fixed_provider_decrypt_and_replay_after_staged_commit() {
    let f = fixture();
    let provider = anchored(&f.receiver_before, 7);
    let state = StateEnvelope::new(7, f.receiver_before.as_bytes().to_vec()).unwrap();
    let result = provider
        .decrypt_unbound(&state, f.ciphertext.as_bytes(), vec![])
        .unwrap();
    assert_eq!(result.plaintext, f.plaintext.as_bytes());
    assert_eq!(
        result.next_state,
        StateEnvelope::new(8, f.receiver_after.as_bytes().to_vec()).unwrap()
    );
    assert_eq!(
        VodozemacProvider::encode_session(&provider.session).unwrap(),
        state.provider
    );
    let committed = anchored(&f.receiver_after, 8);
    assert_eq!(
        committed.decrypt_unbound(&result.next_state, f.ciphertext.as_bytes(), vec![]),
        Err(Error::Provider(ProviderError::Session))
    );
}

#[test]
fn bounded_parsing_rejects_malformed_oversized_and_tampered_input() {
    let f = fixture();
    let provider = anchored(&f.receiver_before, 7);
    let state = StateEnvelope::new(7, f.receiver_before.as_bytes().to_vec()).unwrap();
    for bytes in [
        vec![],
        b"{}".to_vec(),
        vec![b' '; MAX_CIPHERTEXT_BYTES],
        vec![b' '; MAX_CIPHERTEXT_BYTES + 1],
    ] {
        assert_eq!(
            provider.decrypt_unbound(&state, &bytes, vec![]),
            Err(Error::Provider(ProviderError::Ciphertext))
        );
    }
    let mut message: serde_json::Value = serde_json::from_str(&f.ciphertext).unwrap();
    let mut body = message["body"].as_str().unwrap().as_bytes().to_vec();
    let index = body.len() - 8; // A complete base64 sextet inside the MAC.
    body[index] = if body[index] == b'A' { b'B' } else { b'A' };
    message["body"] = String::from_utf8(body).unwrap().into();
    assert_eq!(
        provider.decrypt_unbound(&state, &serde_json::to_vec(&message).unwrap(), vec![]),
        Err(Error::Provider(ProviderError::Session))
    );
    assert_eq!(
        VodozemacProvider::encode_session(&provider.session).unwrap(),
        state.provider
    );
}

#[test]
fn direct_oversized_or_malformed_state_is_rejected_without_restore() {
    let provider = make_provider();
    for bytes in [b"{}".to_vec(), vec![0; MAX_STATE_BYTES + 1]] {
        let state = StateEnvelope {
            version: StateVersion::V1,
            generation: 0,
            provider: bytes,
        };
        assert!(provider.session_for(&state).is_err());
    }
}

#[test]
fn generation_overflow_does_not_create_a_candidate() {
    let provider = make_provider();
    let state = StateEnvelope::new(u64::MAX, vec![]).unwrap();
    assert_eq!(
        VodozemacProvider::next_state(&state, &provider.session),
        Err(Error::StateEncoding)
    );
}

fn make_provider() -> VodozemacProvider {
    let mut bob = Account::new();
    bob.generate_one_time_keys(1);
    let key = *bob.one_time_keys().values().next().unwrap();
    let alice = Account::new();
    VodozemacProvider::new(
        alice
            .create_outbound_session(SessionConfig::version_1(), bob.curve25519_key(), key)
            .unwrap(),
    )
}

#[test]
fn restore_rejects_stale_future_and_same_generation_forks() {
    let mut provider = make_provider();
    provider.generation = 7; // Simulate an anchor installed by a future persistence owner.
    let bytes = VodozemacProvider::encode_session(&provider.session).unwrap();
    let state = StateEnvelope::new(7, bytes).unwrap();
    assert!(provider.session_for(&state).is_ok());
    for generation in [0, 6, 8, u64::MAX] {
        let mut candidate = state.clone();
        candidate.generation = generation;
        assert!(provider.session_for(&candidate).is_err());
    }
    let other = make_provider();
    let fork = StateEnvelope::new(
        7,
        VodozemacProvider::encode_session(&other.session).unwrap(),
    )
    .unwrap();
    assert!(provider.session_for(&fork).is_err());
    let empty = StateEnvelope::new(7, vec![]).unwrap();
    assert!(provider.session_for(&empty).is_err());
}

#[test]
fn restore_rejects_unknown_version_and_noninitial_bootstrap() {
    let provider = make_provider();
    let mut state = StateEnvelope::initial(vec![]).unwrap();
    state.version = crate::StateVersion(9);
    assert_eq!(
        provider.session_for(&state).err(),
        Some(Error::StateVersion {
            found: 9,
            expected: 1
        })
    );
    state.version = crate::StateVersion::V1;
    state.generation = 1;
    assert_eq!(
        provider.session_for(&state).err(),
        Some(Error::StateRollback {
            found: 1,
            expected: 0
        })
    );
}
