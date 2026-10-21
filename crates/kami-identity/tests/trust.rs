mod common;
use common::invitation;
use kami_identity::{
    send_decision, ContactTrust, ContactTrustState, IdentityError, SendDecision, TrustTransition,
    VerificationEvidence,
};

fn verified(invitation: kami_identity::Invitation) -> ContactTrust {
    let pending = ContactTrust::pending(invitation);
    let evidence = VerificationEvidence::independent_comparison(
        &pending,
        pending.candidate().fingerprint(),
        true,
    )
    .unwrap();
    pending
        .transition(TrustTransition::Verify(evidence))
        .unwrap()
}

#[test]
fn bilateral_verification_is_two_independent_local_actions_not_a_remote_assertion() {
    let alice = invitation(1, 2);
    let bob = invitation(3, 4);
    let alice_view = verified(bob.clone());
    let bob_view = ContactTrust::pending(alice.clone());
    assert_eq!(alice_view.state(), ContactTrustState::Verified);
    assert_eq!(bob_view.state(), ContactTrustState::Pending);
    assert_eq!(
        send_decision(
            Some(&bob_view),
            *alice.device_key(),
            Some(alice.binding()),
            Some(&alice.binding().peer_id())
        ),
        SendDecision::Hold(IdentityError::VerificationRequired)
    );
    let evidence =
        VerificationEvidence::independent_comparison(&bob_view, alice.fingerprint(), true).unwrap();
    assert_eq!(
        bob_view
            .transition(TrustTransition::Verify(evidence))
            .unwrap()
            .state(),
        ContactTrustState::Verified
    );
}

#[test]
fn block_is_sticky_and_evidence_is_bound_to_snapshot_candidate_and_action() {
    let first = invitation(1, 2);
    let other = invitation(3, 4);
    let pending = ContactTrust::pending(first.clone());
    assert!(
        VerificationEvidence::independent_comparison(&pending, first.fingerprint(), false).is_err()
    );
    assert!(
        VerificationEvidence::independent_comparison(&pending, other.fingerprint(), true).is_err()
    );
    let stale =
        VerificationEvidence::independent_comparison(&pending, first.fingerprint(), true).unwrap();
    let imported = pending
        .transition(TrustTransition::Observe(other.clone()))
        .unwrap();
    assert_eq!(
        imported.transition(TrustTransition::Verify(stale)),
        Err(IdentityError::Conflict)
    );
    let pin = verified(first);
    let stale =
        VerificationEvidence::independent_comparison(&pin, pin.candidate().fingerprint(), true)
            .unwrap();
    let blocked = pin.transition(TrustTransition::Block).unwrap();
    assert!(VerificationEvidence::independent_comparison(
        &blocked,
        blocked.candidate().fingerprint(),
        true
    )
    .is_err());
    let reimported = blocked.transition(TrustTransition::Observe(other)).unwrap();
    assert_eq!(reimported.state(), ContactTrustState::Blocked);
    let unblocked = reimported.transition(TrustTransition::Unblock).unwrap();
    assert_eq!(unblocked.state(), ContactTrustState::Changed);
    assert_eq!(
        unblocked.transition(TrustTransition::Verify(stale)),
        Err(IdentityError::Conflict)
    );
    let evidence = VerificationEvidence::independent_comparison(
        &unblocked,
        unblocked.candidate().fingerprint(),
        true,
    )
    .unwrap();
    assert_eq!(
        unblocked
            .transition(TrustTransition::Verify(evidence))
            .unwrap()
            .state(),
        ContactTrustState::Verified
    );
    assert_eq!(format!("{pin:?}"), "ContactTrust([REDACTED])");
}

#[test]
fn unknown_unauthenticated_wrong_peer_and_rotated_binding_hold() {
    let descriptor = invitation(1, 2);
    let contact = verified(descriptor.clone());
    let key = *descriptor.device_key();
    let peer = descriptor.binding().peer_id();
    assert_eq!(
        send_decision(None, key, None, None),
        SendDecision::Hold(IdentityError::Unknown)
    );
    assert_eq!(
        send_decision(Some(&contact), key, None, Some(&peer)),
        SendDecision::Hold(IdentityError::Unauthenticated)
    );
    assert_eq!(
        send_decision(Some(&contact), key, Some(descriptor.binding()), None),
        SendDecision::Hold(IdentityError::Unauthenticated)
    );
    let relay = invitation(5, 6).binding().peer_id();
    assert_eq!(
        send_decision(
            Some(&contact),
            key,
            Some(descriptor.binding()),
            Some(&relay)
        ),
        SendDecision::Hold(IdentityError::WrongPeer)
    );
    let rotated = invitation(1, 3);
    assert_eq!(
        send_decision(
            Some(&contact),
            key,
            Some(rotated.binding()),
            Some(&rotated.binding().peer_id())
        ),
        SendDecision::Hold(IdentityError::Changed)
    );
    let changed = contact
        .transition(TrustTransition::Observe(rotated))
        .unwrap();
    assert_eq!(changed.state(), ContactTrustState::Changed);
    assert_eq!(changed.pinned(), Some(&descriptor));
}

#[test]
fn persisted_state_revalidates_invariants_and_generation_overflow_fails() {
    let descriptor = invitation(1, 2);
    let bytes = descriptor.to_bytes();
    assert!(ContactTrust::from_persisted(ContactTrustState::Verified, 1, None, &bytes).is_err());
    assert!(
        ContactTrust::from_persisted(ContactTrustState::Pending, 1, Some(&bytes), &bytes).is_err()
    );
    assert!(ContactTrust::from_persisted(ContactTrustState::Changed, 1, None, &bytes).is_err());
    assert!(ContactTrust::from_persisted(ContactTrustState::Pending, 0, None, &bytes).is_err());
    let last =
        ContactTrust::from_persisted(ContactTrustState::Pending, i64::MAX as u64, None, &bytes)
            .unwrap();
    assert_eq!(
        last.transition(TrustTransition::Block),
        Err(IdentityError::Conflict)
    );
    for state in [
        ContactTrustState::Pending,
        ContactTrustState::Verified,
        ContactTrustState::Changed,
        ContactTrustState::Blocked,
    ] {
        let pin = if state == ContactTrustState::Pending {
            None
        } else {
            Some(bytes.as_slice())
        };
        let restored = ContactTrust::from_persisted(state, 2, pin, &bytes).unwrap();
        assert_eq!(restored.state(), state);
    }
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config { cases: 64, failure_persistence: None, ..Default::default() })]
    #[test]
    fn histories_never_promote_or_replace_a_pin_without_fresh_explicit_verification(events in proptest::collection::vec(0u8..5, 0..40)) {
        let a = invitation(1,2);
        let b = invitation(3,4);
        let mut contact = ContactTrust::pending(a.clone());
        for event in events {
            let old_pin = contact.pinned().cloned();
            let old_state = contact.state();
            let transition = match event {
                0 => TrustTransition::Observe(a.clone()),
                1 => TrustTransition::Observe(b.clone()),
                2 => TrustTransition::Block,
                3 => TrustTransition::Unblock,
                _ => {
                    let Ok(evidence) = VerificationEvidence::independent_comparison(&contact, contact.candidate().fingerprint(), true) else { continue; };
                    TrustTransition::Verify(evidence)
                }
            };
            if let Ok(next) = contact.transition(transition) {
                if event != 4 {
                    proptest::prop_assert_eq!(next.pinned(), old_pin.as_ref());
                    if old_state != ContactTrustState::Verified { proptest::prop_assert_ne!(next.state(), ContactTrustState::Verified); }
                }
                proptest::prop_assert_eq!(next.generation(), contact.generation()+1);
                let decision = send_decision(Some(&next), *a.device_key(), Some(a.binding()), Some(&a.binding().peer_id()));
                if decision == SendDecision::Allow {
                    proptest::prop_assert_eq!(next.state(), ContactTrustState::Verified);
                    proptest::prop_assert_eq!(next.pinned(), Some(&a));
                }
                contact = next;
            }
        }
    }
}

#[test]
fn independent_action_is_required_and_changed_keys_never_redirect_old_sends() {
    let first = invitation(1, 2);
    let pending = ContactTrust::pending(first.clone());
    let peer = first.binding().peer_id();
    assert_eq!(
        send_decision(
            Some(&pending),
            *first.device_key(),
            Some(first.binding()),
            Some(&peer)
        ),
        SendDecision::Hold(IdentityError::VerificationRequired)
    );
    let evidence =
        VerificationEvidence::independent_comparison(&pending, first.fingerprint(), true).unwrap();
    let verified = pending
        .transition(TrustTransition::Verify(evidence))
        .unwrap();
    assert_eq!(verified.state(), ContactTrustState::Verified);
    assert_eq!(
        send_decision(
            Some(&verified),
            *first.device_key(),
            Some(first.binding()),
            Some(&peer)
        ),
        SendDecision::Allow
    );
    let second = invitation(3, 4);
    let changed = verified
        .transition(TrustTransition::Observe(second.clone()))
        .unwrap();
    assert_eq!(changed.state(), ContactTrustState::Changed);
    assert_eq!(changed.pinned(), Some(&first));
    assert_eq!(changed.candidate(), &second);
    assert_eq!(
        send_decision(
            Some(&changed),
            *first.device_key(),
            Some(first.binding()),
            Some(&peer)
        ),
        SendDecision::Hold(IdentityError::Changed)
    );
    let evidence =
        VerificationEvidence::independent_comparison(&changed, second.fingerprint(), true).unwrap();
    let replaced = changed
        .transition(TrustTransition::Verify(evidence))
        .unwrap();
    assert_eq!(
        send_decision(
            Some(&replaced),
            *first.device_key(),
            Some(second.binding()),
            Some(&second.binding().peer_id())
        ),
        SendDecision::Hold(IdentityError::Changed)
    );
}
