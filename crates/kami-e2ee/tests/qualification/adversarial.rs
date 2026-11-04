use super::*;
use vodozemac::{
    olm::{OlmMessage, SessionConfig},
    Curve25519PublicKey,
};
fn raw_session(p: &Pair) -> vodozemac::olm::Session {
    p.alice
        .create_outbound_session(
            SessionConfig::version_1(),
            Curve25519PublicKey::from_bytes(p.a.remote.selected.olm_curve25519),
            Curve25519PublicKey::from_bytes(p.a.remote.selected.prekey_curve25519),
        )
        .unwrap()
}
#[test]
fn authenticated_malformed_inner_has_no_account_session_or_prekey_result() {
    let p = pair();
    let before = serde_json::to_vec(&p.bob.pickle()).unwrap();
    for n in 0..8 {
        let mut raw = raw_session(&p);
        let template = outbound(&p);
        let mut inner = inner_for(&p, &template);
        inner.session_id = raw.session_id();
        let mut bytes = inner.encode().unwrap();
        let base = b"KAMI-INNER-V1\0".len();
        match n {
            0 => {
                bytes.pop();
            }
            1 => bytes.push(0),
            2 => bytes[base] = 2,
            3 => bytes[base + 2] = 2,
            4 => bytes[base + 3] = 99,
            5 => bytes[base + 4] = 99,
            6 => {
                *bytes.last_mut().unwrap() = 0xff;
            }
            7 => bytes = vec![0; 65_000],
            _ => unreachable!(),
        }
        let ciphertext = encode_ciphertext(&raw.encrypt(bytes).unwrap());
        // The commitment is deliberately recomputed for malformed plaintext:
        // authenticating just the outer hash must never bypass inner validation.
        use sha2::{Digest, Sha256};
        let mut hash = Sha256::new();
        hash.update(b"KAMI-INNER-CONTEXT-V1\0");
        // Recover the exact fixture plaintext on an independent staged account.
        let OlmMessage::PreKey(pk) =
            OlmMessage::from_parts(usize::from(ciphertext[0]), &ciphertext[1..]).unwrap()
        else {
            panic!()
        };
        let mut independent = Account::from_pickle(p.bob.pickle());
        let decrypted = independent
            .create_inbound_session(SessionConfig::version_1(), p.alice.curve25519_key(), &pk)
            .unwrap();
        hash.update(&decrypted.plaintext);
        let commitment = hash.finalize();
        let frame = outer(&inner, &ciphertext, &commitment);
        let result = inbound(&p, &frame, &inner);
        assert!(
            matches!(result, Err(crate::Error::AuthenticationRejected)),
            "case {n}"
        );
        assert_eq!(before, serde_json::to_vec(&p.bob.pickle()).unwrap());
    }
}
#[test]
fn prekey_choice_kind_missing_key_and_already_consumed_key_fail_closed() {
    for fallback in [false, true] {
        let mut p = pair();
        if fallback {
            p.bob.generate_fallback_key();
            let key = *p.bob.fallback_key().values().next().unwrap();
            p.b.local.selected.prekey_kind = OlmPrekeyKind::Fallback;
            p.b.local.selected.prekey_curve25519 = *key.as_bytes();
            p.b.local.binding = sign_selection(&p.b.local.selected, 8);
            p.a.remote = p.b.local.clone();
        }
        p.bob.mark_keys_as_published(); // Public inventory getters now omit keys.
        let a = outbound(&p);
        let inner = inner_for(&p, &a);
        let sent = send(&p, &a, &inner);
        let good = inbound(&p, &sent.frame, &inner).unwrap();
        assert_eq!(
            good.account.stored_one_time_key_count(),
            if fallback { 1 } else { 0 }
        );
        if !fallback {
            assert!(BoundSession::stage_inbound(
                &good.account,
                p.b.clone(),
                &sent.frame,
                &expected(&inner),
                Some(&p.trust_a),
                Some(&inner.sender_peer),
                1000
            )
            .is_err());
        }
        // Signed but wrong kind cannot silently pick the other prekey family.
        p.b.local.selected.prekey_kind = if fallback {
            OlmPrekeyKind::OneTime
        } else {
            OlmPrekeyKind::Fallback
        };
        p.b.local.binding = sign_selection(&p.b.local.selected, 8);
        p.a.remote = p.b.local.clone();
        let a = outbound(&p);
        let inner = inner_for(&p, &a);
        let sent = send(&p, &a, &inner);
        assert!(inbound(&p, &sent.frame, &inner).is_err());
        // Signed but unavailable key is not a provision/selection instruction.
        p.b.local.selected.prekey_curve25519 = *Account::new().curve25519_key().as_bytes();
        p.b.local.binding = sign_selection(&p.b.local.selected, 8);
        p.a.remote = p.b.local.clone();
        let a = outbound(&p);
        let inner = inner_for(&p, &a);
        let sent = send(&p, &a, &inner);
        assert!(inbound(&p, &sent.frame, &inner).is_err());
    }
}
#[test]
fn repeated_prekey_routes_existing_session_and_checks_wrapper_keys() {
    let p = pair();
    let a = outbound(&p);
    let first = inner_for(&p, &a);
    let sent = send(&p, &a, &first);
    let b = inbound(&p, &sent.frame, &first).unwrap();
    let mut second = first.clone();
    second.context.message = kami_core::MessageId::from_u128(99);
    second.sequence = Some(kami_core::SenderSequence::new(2));
    let sent2 = send(&p, &sent.session, &second);
    let frame = kami_protocol::decode_frame(&sent2.frame).unwrap();
    assert_eq!(frame.protected_payload()[0], 0);
    assert!(b
        .session
        .stage_receive(
            &sent2.frame,
            &expected(&second),
            Some(&p.trust_a),
            Some(&second.sender_peer),
            1000
        )
        .is_ok());
    let mut ciphertext = frame.protected_payload().to_vec();
    let mut changed_sequence = second.clone();
    changed_sequence.sequence = Some(kami_core::SenderSequence::new(99));
    let substituted = outer(
        &changed_sequence,
        &ciphertext,
        frame.authentication_context(),
    );
    // Fails after successful staged decryption, not at the provider MAC check.
    assert!(b
        .session
        .stage_receive(
            &substituted,
            &expected(&changed_sequence),
            Some(&p.trust_a),
            Some(&second.sender_peer),
            1000
        )
        .is_err());
    // Prekey: type byte, version byte, key-tag, length, OTK[32]. MAC does
    // not cover wrapper routing keys; the anchored wrapper check must reject.
    ciphertext[4] ^= 1;
    let changed = outer(&second, &ciphertext, frame.authentication_context());
    assert!(b
        .session
        .stage_receive(
            &changed,
            &expected(&second),
            Some(&p.trust_a),
            Some(&second.sender_peer),
            1000
        )
        .is_err());
    // A wrong-session MAC-valid prekey cannot be routed onto an existing session.
    let other = outbound(&p);
    let different = inner_for(&p, &other);
    let other_sent = send(&p, &other, &different);
    assert!(b
        .session
        .stage_receive(
            &other_sent.frame,
            &expected(&different),
            Some(&p.trust_a),
            Some(&different.sender_peer),
            1000
        )
        .is_err());
    assert!(b
        .session
        .stage_receive(
            &sent2.frame,
            &expected(&second),
            Some(&p.trust_a),
            Some(&second.sender_peer),
            1000
        )
        .is_ok());
}
#[test]
fn provider_and_outer_unknown_duplicate_noncanonical_truncated_and_tampered_inputs_reject() {
    let p = pair();
    let a = outbound(&p);
    let inner = inner_for(&p, &a);
    let sent = send(&p, &a, &inner);
    let frame = kami_protocol::decode_frame(&sent.frame).unwrap();
    let ct = frame.protected_payload();
    for n in [0, 1, 2, ct.len() / 2, ct.len() - 1] {
        let bad = outer(&inner, &ct[..n.max(1)], frame.authentication_context());
        assert!(inbound(&p, &bad, &inner).is_err());
    }
    for n in [0, 1, 4, ct.len() / 2, ct.len() - 1] {
        let mut ct = ct.to_vec();
        ct[n] ^= 0x80;
        let bad = outer(&inner, &ct, frame.authentication_context());
        assert!(inbound(&p, &bad, &inner).is_err());
    }
    for extra in [vec![0x28, 1], ct[2..36].to_vec(), vec![0]] {
        let mut changed = ct.to_vec();
        changed.extend(extra);
        let bad = outer(&inner, &changed, frame.authentication_context());
        assert!(inbound(&p, &bad, &inner).is_err());
    }
    for n in [0, 1, 2, sent.frame.len() / 2, sent.frame.len() - 1] {
        assert!(inbound(&p, &sent.frame[..n], &inner).is_err());
    }
    for extra in [vec![0x68, 1], vec![8, 1]] {
        let mut bad = sent.frame.clone();
        bad.extend(extra);
        assert!(inbound(&p, &bad, &inner).is_err());
    }
    let mut nonminimal = sent.frame.clone();
    nonminimal[1] = 0x81;
    nonminimal.insert(2, 0);
    assert!(inbound(&p, &nonminimal, &inner).is_err());
    let short = outer(&inner, ct, &[1; 16]);
    assert!(inbound(&p, &short, &inner).is_err());
    assert_eq!(
        format!("{}", crate::Error::AuthenticationRejected),
        "authenticated envelope rejected"
    );
    assert!(inbound(&p, &sent.frame, &inner).is_ok());
}
