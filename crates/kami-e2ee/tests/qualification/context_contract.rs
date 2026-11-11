//! WP-A regression tests exercise real Olm composition, not hash authentication.
use super::*;
use crate::{MessageId, QuarantinedSession};
use kami_core::SenderSequence;

#[test]
fn recomputed_outer_hash_cannot_rebind_authenticated_plaintext() {
    let p = pair();
    let source = outbound(&p);
    let inner = inner_for(&p, &source);
    let sent = send(&p, &source, &inner);
    let frame = kami_protocol::decode_frame(&sent.frame).unwrap();
    let before = serde_json::to_vec(&p.bob.pickle()).unwrap();
    for n in 0..7 {
        let mut substituted = inner.clone();
        match n {
            0 => substituted.context.conversation = ConversationId::from_u128(99),
            1 => substituted.context.message = MessageId::from_u128(99),
            2 => substituted.context.sender = DeviceId::from_bytes([99; 32]),
            3 => substituted.context.recipient = DeviceId::from_bytes([99; 32]),
            4 => substituted.sequence = Some(SenderSequence::new(99)),
            5 => substituted.created_at_ms += 1,
            6 => substituted.payload = b"attacker replacement".to_vec(),
            _ => unreachable!(),
        }
        // An attacker can hash any replacement; only the original inner bytes
        // are Olm-authenticated. Even matching attacker expectations must fail.
        let forged = outer(
            &substituted,
            frame.protected_payload(),
            &substituted.commitment().unwrap(),
        );
        assert!(inbound(&p, &forged, &substituted).is_err(), "field {n}");
        assert_eq!(serde_json::to_vec(&p.bob.pickle()).unwrap(), before);
    }
    assert!(inbound(&p, &sent.frame, &inner).is_ok());
}

#[test]
fn supplied_expectations_do_not_override_authenticated_context_or_advance_state() {
    let p = pair();
    let source = outbound(&p);
    let inner = inner_for(&p, &source);
    let sent = send(&p, &source, &inner);
    let before = serde_json::to_vec(&p.bob.pickle()).unwrap();
    for n in 0..4 {
        let mut wrong = expected(&inner);
        match n {
            0 => wrong.message_id = MessageId::from_u128(99),
            1 => wrong.conversation_id = ConversationId::from_u128(99),
            2 => wrong.sender = DeviceId::from_bytes([99; 32]),
            3 => wrong.recipient = DeviceId::from_bytes([99; 32]),
            _ => unreachable!(),
        }
        assert!(BoundSession::stage_inbound(
            &p.bob,
            p.b.clone(),
            &sent.frame,
            &wrong,
            Some(&p.trust_a),
            Some(&inner.sender_peer),
            1000,
        )
        .is_err());
        assert_eq!(serde_json::to_vec(&p.bob.pickle()).unwrap(), before);
    }
    assert!(inbound(&p, &sent.frame, &inner).is_ok());
}

#[test]
fn receive_finalization_rechecks_supplied_trust_peer_and_expiry_without_advancement() {
    let p = pair();
    let source = outbound(&p);
    let first = inner_for(&p, &source);
    let sent = send(&p, &source, &first);
    let received = inbound(&p, &sent.frame, &first).unwrap();
    let mut second = first.clone();
    second.context.message = MessageId::from_u128(99);
    second.sequence = Some(SenderSequence::new(2));
    let next = send(&p, &sent.session, &second);
    let before = received.session.snapshot(1, 2).unwrap().digest();
    let blocked = p.trust_a.transition(TrustTransition::Block).unwrap();
    let newer = p
        .trust_a
        .transition(TrustTransition::Observe(p.trust_a.candidate().clone()))
        .unwrap();
    for (trust, peer, now) in [
        (None, Some(&second.sender_peer), 1000),
        (Some(&blocked), Some(&second.sender_peer), 1000),
        (Some(&newer), Some(&second.sender_peer), 1000),
        (Some(&p.trust_a), None, 1000),
        (Some(&p.trust_a), Some(&second.recipient_peer), 1000),
        (Some(&p.trust_a), Some(&second.sender_peer), 2000),
    ] {
        let stage = received
            .session
            .stage_receive(
                &next.frame,
                &expected(&second),
                Some(&p.trust_a),
                Some(&second.sender_peer),
                1000,
            )
            .unwrap();
        assert!(stage
            .finalize(&received.account, &received.session, 2, 1, trust, peer, now)
            .is_err());
        assert_eq!(received.session.snapshot(1, 2).unwrap().digest(), before);
    }
    let stage = received
        .session
        .stage_receive(
            &next.frame,
            &expected(&second),
            Some(&p.trust_a),
            Some(&second.sender_peer),
            1000,
        )
        .unwrap();
    assert!(stage
        .finalize(
            &received.account,
            &received.session,
            2,
            1,
            Some(&p.trust_a),
            Some(&second.sender_peer),
            1000,
        )
        .is_ok());
}

#[test]
fn retry_rechecks_supplied_trust_and_peer_and_retains_exact_ciphertext() {
    let p = pair();
    let peer = p.a.remote.selected.publisher_peer;
    let source = QuarantinedSession::outbound(
        Account::from_pickle(p.alice.pickle()),
        p.a.local.clone(),
        p.a.remote.clone(),
        p.a.conversation,
        &p.trust_b,
        &peer,
        1000,
    )
    .unwrap();
    let (commit, proposed) = source
        .prepare_text(
            MessageId::from_u128(123),
            SenderSequence::new(1),
            1000,
            "bounded context",
            &p.trust_b,
            &peer,
            1000,
        )
        .unwrap();
    let material = commit.into_material();
    let frame = material.frame().to_vec();
    let digest = material.next_digest();
    let blocked = p.trust_b.transition(TrustTransition::Block).unwrap();
    let newer = p
        .trust_b
        .transition(TrustTransition::Observe(p.trust_b.candidate().clone()))
        .unwrap();
    let (_, other) = endpoint(&p.alice, 9);
    let changed = p
        .trust_b
        .transition(TrustTransition::Observe(other.candidate().clone()))
        .unwrap();
    for trust in [&blocked, &newer, &changed] {
        assert!(proposed.prepare_retry(trust, &peer, 1000).is_err());
    }
    assert!(proposed
        .prepare_retry(&p.trust_b, &p.a.local.selected.publisher_peer, 1000)
        .is_err());
    assert!(proposed.prepare_retry(&p.trust_b, &peer, 2000).is_err());
    for _ in 0..2 {
        let retry = proposed
            .prepare_retry(&p.trust_b, &peer, 1000)
            .unwrap()
            .into_material();
        assert_eq!(retry.frame(), frame);
        assert_eq!(retry.next_digest(), digest);
    }
    // This only proves checks against supplied values. An old matching snapshot
    // is not independently fresh: runtime/storage must serialize that decision.
    assert!(source.prepare_retry(&p.trust_b, &peer, 1000).is_err());
}
