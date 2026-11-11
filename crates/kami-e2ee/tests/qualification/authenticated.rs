use crate::authenticated::{Anchor, BoundSession, Endpoint};
#[path = "adversarial.rs"]
mod adversarial;
#[path = "context_contract.rs"]
mod context_contract;
#[path = "frame_limits.rs"]
mod frame_limits;
#[path = "receipts.rs"]
mod receipts;
use crate::envelope::Direction;
use kami_core::{ConversationId, DeviceId};
use kami_identity::{
    ContactTrust, Invitation, OlmBindingData, OlmBindingProof, OlmPrekeyKind, PeerBindingProof,
    TrustTransition, VerificationEvidence,
};
use libp2p_identity::Keypair;
use vodozemac::olm::Account;

#[test]
fn wp3_private_snapshot_roundtrip_and_restore_anchor_fail_closed() {
    use crate::persistence::{AccountSnapshot, RestoreAuthority};
    let p = pair();
    let session = outbound(&p);
    let account = AccountSnapshot::capture(&p.alice, &p.a.local, 1).unwrap();
    let digest = account.digest();
    let restored = account
        .restore(&p.a.local, 1, RestoreAuthority::InProcess(digest))
        .unwrap();
    assert_eq!(restored.identity_keys(), p.alice.identity_keys());
    assert!(matches!(
        account.restore(&p.a.local, 1, RestoreAuthority::ProfileRestore),
        Err(crate::Error::RollbackAnchorUnsupported)
    ));
    assert!(account
        .restore(&p.a.local, 2, RestoreAuthority::InProcess(digest))
        .is_err());
    assert!(account
        .restore(&p.a.local, 1, RestoreAuthority::InProcess([0; 32]))
        .is_err());
    let snapshot = session.snapshot(1, 1).unwrap();
    let expected_digest = snapshot.digest();
    let resumed = BoundSession::restore(
        &snapshot,
        p.a.clone(),
        &p.alice,
        1,
        1,
        &session.session_id(),
        RestoreAuthority::InProcess(expected_digest),
        Some(&p.trust_b),
        Some(&p.a.remote.selected.publisher_peer),
        1000,
    )
    .unwrap();
    assert_eq!(resumed.session_id(), session.session_id());
    assert!(BoundSession::restore(
        &snapshot,
        p.a.clone(),
        &p.alice,
        1,
        1,
        &session.session_id(),
        RestoreAuthority::ProfileRestore,
        Some(&p.trust_b),
        Some(&p.a.remote.selected.publisher_peer),
        1000
    )
    .is_err());
    assert!(BoundSession::restore(
        &snapshot,
        p.b.clone(),
        &p.bob,
        1,
        1,
        &session.session_id(),
        RestoreAuthority::InProcess(expected_digest),
        Some(&p.trust_a),
        Some(&p.b.remote.selected.publisher_peer),
        1000
    )
    .is_err());
    assert!(crate::persistence::SessionSnapshot::parse(&vec![
        0;
        crate::state::MAX_STATE_BYTES + 256
    ])
    .is_err());
    assert!(crate::persistence::SessionSnapshot::parse(b"bad").is_err());
}

#[test]
fn wp3_snapshot_bounds_versions_and_prekey_consumption_roundtrip() {
    use crate::persistence::{AccountSnapshot, RestoreAuthority, SessionSnapshot};
    let p = pair();
    let a = outbound(&p);
    let inner = inner_for(&p, &a);
    let sent = send(&p, &a, &inner);
    let received = inbound(&p, &sent.frame, &inner).unwrap();
    let account = AccountSnapshot::capture(&received.account, &p.b.local, 2).unwrap();
    let parsed = AccountSnapshot::parse(account.bytes()).unwrap();
    let restored = parsed
        .restore(&p.b.local, 2, RestoreAuthority::InProcess(account.digest()))
        .unwrap();
    assert!(restored.one_time_keys().is_empty());
    assert!(!p.bob.one_time_keys().is_empty());
    // Even coherent old bytes plus their old digest are not external rollback authority.
    let old = AccountSnapshot::capture(&p.bob, &p.b.local, 1).unwrap();
    assert!(matches!(
        AccountSnapshot::parse(old.bytes()).unwrap().restore(
            &p.b.local,
            1,
            RestoreAuthority::ProfileRestore
        ),
        Err(crate::Error::RollbackAnchorUnsupported)
    ));
    let snapshot = received.session.snapshot(1, 2).unwrap();
    let parsed = SessionSnapshot::parse(snapshot.bytes()).unwrap();
    let resumed = BoundSession::restore(
        &parsed,
        p.b.clone(),
        &restored,
        1,
        2,
        &received.session.session_id(),
        RestoreAuthority::InProcess(snapshot.digest()),
        Some(&p.trust_a),
        Some(&p.b.remote.selected.publisher_peer),
        1000,
    )
    .unwrap();
    assert_eq!(resumed.session_id(), received.session.session_id());
    assert!(resumed
        .stage_receive(
            &sent.frame,
            &expected(&inner),
            Some(&p.trust_a),
            Some(&inner.sender_peer),
            1000
        )
        .is_err());
    let mut second = inner.clone();
    second.context.message = kami_core::MessageId::from_u128(2);
    let sent2 = send(&p, &sent.session, &second);
    assert!(resumed
        .stage_receive(
            &sent2.frame,
            &expected(&second),
            Some(&p.trust_a),
            Some(&inner.sender_peer),
            1000
        )
        .is_ok());
    for index in [0, 8, 9, 58, 59, 63, 65] {
        let mut corrupt = snapshot.bytes().to_vec();
        corrupt[index] = 255;
        assert!(
            SessionSnapshot::parse(&corrupt).is_err(),
            "header byte {index}"
        );
    }
    for len in 0..66 {
        assert!(SessionSnapshot::parse(&snapshot.bytes()[..len]).is_err());
    }
    let mut trailing = snapshot.bytes().to_vec();
    trailing.push(0);
    assert!(SessionSnapshot::parse(&trailing).is_err());
    assert!(AccountSnapshot::parse(snapshot.bytes()).is_err());
    for (generation, account_generation) in [(0, 1), (1, 0), (u64::MAX, 1), (1, u64::MAX)] {
        assert!(a.snapshot(generation, account_generation).is_err());
    }
    // Same-counter substitutions are rejected against a separately retained digest.
    let mut fork = snapshot.bytes().to_vec();
    fork[26] ^= 1;
    let fork = SessionSnapshot::parse(&fork).unwrap();
    assert!(fork
        .restore(
            1,
            2,
            [0; 32],
            RestoreAuthority::InProcess(snapshot.digest())
        )
        .is_err());
}

#[test]
fn wp2b_stage_finalization_preserves_frame_and_revalidates_fresh_trust() {
    let p = pair();
    let source = outbound(&p);
    let inner = inner_for(&p, &source);
    let stage = send(&p, &source, &inner);
    let frame = stage.frame.clone();
    let commit = stage
        .finalize(
            &p.alice,
            &source,
            1,
            0,
            Some(&p.trust_b),
            Some(&p.a.remote.selected.publisher_peer),
            1000,
        )
        .unwrap();
    assert_eq!(commit.facts().message_id(), inner.context.message);
    let material = commit.into_material();
    assert_eq!(material.frame(), frame);
    assert!(!material.incoming());
    let blocked = p.trust_b.transition(TrustTransition::Block).unwrap();
    assert!(send(&p, &source, &inner)
        .finalize(
            &p.alice,
            &source,
            1,
            0,
            Some(&blocked),
            Some(&p.a.remote.selected.publisher_peer),
            1000
        )
        .is_err());
    assert!(send(&p, &source, &inner)
        .finalize(&p.alice, &source, 1, 0, Some(&p.trust_b), None, 1000)
        .is_err());
    assert!(send(&p, &source, &inner)
        .finalize(
            &p.alice,
            &source,
            1,
            0,
            Some(&p.trust_b),
            Some(&p.a.remote.selected.publisher_peer),
            2000
        )
        .is_err());
}

#[test]
fn wp2b_finalization_rejects_wrong_source_branch_and_receipt() {
    let p = pair();
    let source = outbound(&p);
    let inner = inner_for(&p, &source);
    let unrelated = outbound(&p);
    assert!(send(&p, &source, &inner)
        .finalize(
            &p.alice,
            &unrelated,
            1,
            0,
            Some(&p.trust_b),
            Some(&p.a.remote.selected.publisher_peer),
            1000
        )
        .is_err());
    let sent = send(&p, &source, &inner);
    let mut fork = Account::from_pickle(p.bob.pickle());
    fork.generate_one_time_keys(1);
    assert!(inbound(&p, &sent.frame, &inner)
        .unwrap()
        .finalize(
            &fork,
            Some(&p.trust_a),
            Some(&p.b.remote.selected.publisher_peer),
            1000
        )
        .is_err());
    let commit = inbound(&p, &sent.frame, &inner)
        .unwrap()
        .finalize(
            &p.bob,
            Some(&p.trust_a),
            Some(&p.b.remote.selected.publisher_peer),
            1000,
        )
        .unwrap();
    let m = commit.into_material();
    assert_eq!(m.frame(), sent.frame);
    assert!(m.incoming());
    assert!(m.advances_account());
    m.into_snapshots().consume(
        |old: zeroize::Zeroizing<Vec<u8>>, next, previous, session| {
            assert!(previous.is_none());
            assert!(crate::persistence::AccountSnapshot::parse(&old).is_ok());
            assert!(crate::persistence::AccountSnapshot::parse(&next.unwrap()).is_ok());
            assert!(crate::persistence::SessionSnapshot::parse(&session).is_ok());
        },
    );
    let mut receipt = inner.clone();
    receipt.kind = kami_protocol::MessageKind::DeviceDelivered;
    receipt.sequence = None;
    receipt.receipt_target = Some(kami_core::MessageId::from_u128(99));
    receipt.payload.clear();
    assert!(send(&p, &source, &receipt)
        .finalize(
            &p.alice,
            &source,
            1,
            0,
            Some(&p.trust_b),
            Some(&p.a.remote.selected.publisher_peer),
            1000
        )
        .is_err());
}

// Independent fixture signing with public provider APIs, not a production key API.
fn endpoint(account: &Account, seed: u8) -> (Endpoint, ContactTrust) {
    let device = Keypair::ed25519_from_bytes([seed; 32]).unwrap();
    let public = device.public().try_into_ed25519().unwrap().to_bytes();
    let transport = Keypair::ed25519_from_bytes([seed + 10; 32])
        .unwrap()
        .public();
    let peer = transport.to_peer_id();
    let mut fields = Vec::new();
    for bytes in [transport.encode_protobuf(), peer.to_bytes()] {
        fields.extend((bytes.len() as u16).to_be_bytes());
        fields.extend(bytes);
    }
    let mut transcript = b"kami-peer-binding-v1\0".to_vec();
    transcript.extend(&fields);
    let mut wire = vec![1];
    wire.extend(fields);
    wire.extend(device.sign(&transcript).unwrap());
    let peer_binding = PeerBindingProof::parse_bounded(&wire).unwrap();
    let invitation = Invitation::new(public, peer_binding.clone()).unwrap();
    let pending = ContactTrust::pending(invitation);
    let evidence = VerificationEvidence::independent_comparison(
        &pending,
        pending.candidate().fingerprint(),
        true,
    )
    .unwrap();
    let trust = pending
        .transition(TrustTransition::Verify(evidence))
        .unwrap();
    let prekey = *account.one_time_keys().values().next().unwrap();
    let selected = OlmBindingData {
        publisher_peer: peer,
        olm_ed25519: *account.ed25519_key().as_bytes(),
        olm_curve25519: *account.curve25519_key().as_bytes(),
        prekey_kind: OlmPrekeyKind::OneTime,
        prekey_reference: vec![seed],
        prekey_curve25519: *prekey.as_bytes(),
        binding_generation: 1,
        bundle_generation: 1,
        not_before_ms: 1,
        expires_at_ms: 2000,
    };
    let binding = sign_selection(&selected, seed);
    (
        Endpoint {
            device: DeviceId::from_bytes(public),
            peer_binding,
            binding,
            selected,
        },
        trust,
    )
}
fn sign_selection(selected: &OlmBindingData, seed: u8) -> OlmBindingProof {
    let device = Keypair::ed25519_from_bytes([seed; 32]).unwrap();
    let public = device.public().try_into_ed25519().unwrap().to_bytes();
    let mut wire = b"kami-olm-binding-v1\0".to_vec();
    wire.push(1);
    wire.extend(public);
    let peer = selected.publisher_peer.to_bytes();
    wire.extend((peer.len() as u16).to_be_bytes());
    wire.extend(peer);
    wire.extend(selected.olm_ed25519);
    wire.extend(selected.olm_curve25519);
    wire.push(selected.prekey_kind as u8);
    wire.extend((selected.prekey_reference.len() as u16).to_be_bytes());
    wire.extend(&selected.prekey_reference);
    wire.extend(selected.prekey_curve25519);
    for n in [
        selected.binding_generation,
        selected.bundle_generation,
        selected.not_before_ms,
        selected.expires_at_ms,
    ] {
        wire.extend(n.to_be_bytes());
    }
    wire.extend(device.sign(&wire).unwrap());
    OlmBindingProof::parse_bounded(&wire).unwrap()
}
struct Pair {
    alice: Account,
    bob: Account,
    a: Anchor,
    b: Anchor,
    trust_a: ContactTrust,
    trust_b: ContactTrust,
}
fn pair() -> Pair {
    let mut alice = Account::new();
    alice.generate_one_time_keys(1);
    let mut bob = Account::new();
    bob.generate_one_time_keys(1);
    let (a, trust_a) = endpoint(&alice, 7);
    let (b, trust_b) = endpoint(&bob, 8);
    let a_anchor = Anchor {
        local: a.clone(),
        remote: b.clone(),
        conversation: ConversationId::from_u128(1),
        trust_generation: trust_b.generation(),
    };
    let b_anchor = Anchor {
        local: b,
        remote: a,
        conversation: ConversationId::from_u128(1),
        trust_generation: trust_a.generation(),
    };
    Pair {
        alice,
        bob,
        a: a_anchor,
        b: b_anchor,
        trust_a,
        trust_b,
    }
}
fn expected(inner: &crate::envelope::InnerEnvelopeV1) -> kami_protocol::ExpectedContext {
    kami_protocol::ExpectedContext {
        message_id: inner.context.message,
        conversation_id: inner.context.conversation,
        sender: inner.context.sender,
        recipient: inner.context.recipient,
        receipt_target: inner.receipt_target,
    }
}
#[test]
fn authenticated_first_message_roundtrip_stages_account_and_session() {
    let p = pair();
    let session = BoundSession::outbound(
        &p.alice,
        p.a.clone(),
        Some(&p.trust_b),
        Some(&p.a.remote.selected.publisher_peer),
        1000,
    )
    .unwrap();
    let mut inner = crate::envelope_tests::sample();
    inner.context.sender = p.a.local.device;
    inner.context.recipient = p.a.remote.device;
    inner.session_id = session.session_id();
    inner.sender_peer = p.a.local.selected.publisher_peer;
    inner.recipient_peer = p.a.remote.selected.publisher_peer;
    inner.sender_binding = p.a.local.digest();
    inner.recipient_binding = p.a.remote.digest();
    inner.direction = Direction::InitiatorToResponder;
    let sent = session
        .stage_send(&inner, Some(&p.trust_b), Some(&inner.recipient_peer), 1000)
        .unwrap();
    let before = serde_json::to_vec(&p.bob.pickle()).unwrap();
    let received = BoundSession::stage_inbound(
        &p.bob,
        p.b.clone(),
        &sent.frame,
        &expected(&inner),
        Some(&p.trust_a),
        Some(&inner.sender_peer),
        1000,
    )
    .unwrap();
    assert_eq!(received.inner, inner);
    assert_eq!(serde_json::to_vec(&p.bob.pickle()).unwrap(), before);
    assert!(received.account.one_time_keys().is_empty());
    assert!(!p.bob.one_time_keys().is_empty());
}

fn inner_for(p: &Pair, session: &BoundSession) -> crate::envelope::InnerEnvelopeV1 {
    let mut inner = crate::envelope_tests::sample();
    inner.context.sender = p.a.local.device;
    inner.context.recipient = p.a.remote.device;
    inner.session_id = session.session_id();
    inner.sender_peer = p.a.local.selected.publisher_peer;
    inner.recipient_peer = p.a.remote.selected.publisher_peer;
    inner.sender_binding = p.a.local.digest();
    inner.recipient_binding = p.a.remote.digest();
    inner
}
fn outbound(p: &Pair) -> BoundSession {
    BoundSession::outbound(
        &p.alice,
        p.a.clone(),
        Some(&p.trust_b),
        Some(&p.a.remote.selected.publisher_peer),
        1000,
    )
    .unwrap()
}
fn send(
    p: &Pair,
    s: &BoundSession,
    i: &crate::envelope::InnerEnvelopeV1,
) -> crate::authenticated::SendStage {
    s.stage_send(
        i,
        Some(&p.trust_b),
        Some(&p.a.remote.selected.publisher_peer),
        1000,
    )
    .unwrap()
}
fn inbound(
    p: &Pair,
    bytes: &[u8],
    i: &crate::envelope::InnerEnvelopeV1,
) -> Result<crate::authenticated::InboundStage, crate::Error> {
    BoundSession::stage_inbound(
        &p.bob,
        p.b.clone(),
        bytes,
        &expected(i),
        Some(&p.trust_a),
        Some(&p.b.remote.selected.publisher_peer),
        1000,
    )
}
fn outer(inner: &crate::envelope::InnerEnvelopeV1, payload: &[u8], commitment: &[u8]) -> Vec<u8> {
    use kami_protocol::{encode_frame, FrameInput, StructurallyValidFrame};
    encode_frame(
        &StructurallyValidFrame::new(FrameInput {
            message_id: inner.context.message,
            conversation_id: inner.context.conversation,
            sender: inner.context.sender,
            recipient: inner.context.recipient,
            kind: inner.kind,
            sequence: inner.sequence,
            receipt_target: inner.receipt_target,
            created_at_ms: inner.created_at_ms,
            protected_payload: payload,
            authentication_context: commitment,
        })
        .unwrap(),
    )
    .unwrap()
}
fn encode_ciphertext(m: &vodozemac::olm::OlmMessage) -> Vec<u8> {
    let (kind, b) = m.to_parts();
    let mut out = vec![kind as u8];
    out.extend(b);
    out
}
#[test]
fn every_outer_field_mismatch_discards_first_message_account_and_prekey_effects() {
    let p = pair();
    let s = outbound(&p);
    let inner = inner_for(&p, &s);
    let sent = send(&p, &s, &inner);
    let frame = kami_protocol::decode_frame(&sent.frame).unwrap();
    let before = serde_json::to_vec(&p.bob.pickle()).unwrap();
    for n in 0..9 {
        let mut changed = inner.clone();
        match n {
            0 => changed.context.conversation = ConversationId::from_u128(99),
            1 => changed.context.message = kami_core::MessageId::from_u128(99),
            2 => changed.context.sender = DeviceId::from_bytes([99; 32]),
            3 => changed.context.recipient = DeviceId::from_bytes([99; 32]),
            4 => changed.sequence = Some(kami_core::SenderSequence::new(99)),
            5 => changed.created_at_ms += 1,
            6 => {
                changed.kind = kami_protocol::MessageKind::DeviceDelivered;
                changed.sequence = None;
                changed.receipt_target = Some(kami_core::MessageId::from_u128(99));
            }
            _ => {}
        }
        let mut commitment = frame.authentication_context().to_vec();
        if n == 7 {
            commitment[0] ^= 1;
        }
        let payload = if n == 8 {
            let mut v = frame.protected_payload().to_vec();
            v[10] ^= 1;
            v
        } else {
            frame.protected_payload().to_vec()
        };
        let bytes = outer(&changed, &payload, &commitment);
        // Even if external expectations are taken from substituted outer metadata,
        // the encrypted inner correspondence MUST detect the substitution.
        assert!(inbound(&p, &bytes, &changed).is_err(), "outer field {n}");
        assert_eq!(serde_json::to_vec(&p.bob.pickle()).unwrap(), before);
    }
    assert!(inbound(&p, &sent.frame, &inner).is_ok());
}
#[test]
fn valid_olm_mac_cannot_substitute_trusted_inner_session_direction_devices_peers_or_bindings() {
    use vodozemac::{olm::SessionConfig, Curve25519PublicKey};
    let p = pair();
    for n in 0..9 {
        let mut raw = p
            .alice
            .create_outbound_session(
                SessionConfig::version_1(),
                Curve25519PublicKey::from_bytes(p.a.remote.selected.olm_curve25519),
                Curve25519PublicKey::from_bytes(p.a.remote.selected.prekey_curve25519),
            )
            .unwrap();
        let s = outbound(&p);
        let mut inner = inner_for(&p, &s);
        inner.session_id = raw.session_id();
        match n {
            0 => inner.session_id = "A".repeat(43),
            1 => inner.direction = Direction::ResponderToInitiator,
            2 => inner.context.sender = DeviceId::from_bytes([99; 32]),
            3 => inner.context.recipient = DeviceId::from_bytes([99; 32]),
            4 => inner.context.conversation = ConversationId::from_u128(99),
            5 => inner.sender_peer = inner.recipient_peer,
            6 => inner.recipient_peer = inner.sender_peer,
            7 => inner.sender_binding[0] ^= 1,
            8 => inner.recipient_binding[0] ^= 1,
            _ => unreachable!(),
        }
        let bytes = inner.encode().unwrap();
        let message = raw.encrypt(&bytes).unwrap();
        let ciphertext = encode_ciphertext(&message);
        let frame = outer(&inner, &ciphertext, &inner.commitment().unwrap());
        let before = serde_json::to_vec(&p.bob.pickle()).unwrap();
        assert!(inbound(&p, &frame, &inner).is_err(), "inner field {n}");
        assert_eq!(before, serde_json::to_vec(&p.bob.pickle()).unwrap());
    }
}
#[test]
fn direct_and_relay_require_authenticated_end_peer_not_relay_or_indirect_prekeys() {
    let p = pair();
    let end = p.a.remote.selected.publisher_peer;
    let relay = Keypair::ed25519_from_bytes([88; 32])
        .unwrap()
        .public()
        .to_peer_id();
    for observed in [None, Some(&relay), Some(&p.a.local.selected.publisher_peer)] {
        assert!(
            BoundSession::outbound(&p.alice, p.a.clone(), Some(&p.trust_b), observed, 1000)
                .is_err()
        );
    }
    // Path-independent API receives actual secure end-peer, not a relay claim.
    for _path in ["direct", "relay"] {
        assert!(
            BoundSession::outbound(&p.alice, p.a.clone(), Some(&p.trust_b), Some(&end), 1000)
                .is_ok()
        );
    }
    let s = outbound(&p);
    let inner = inner_for(&p, &s);
    let sent = send(&p, &s, &inner);
    for observed in [None, Some(&relay)] {
        assert!(BoundSession::stage_inbound(
            &p.bob,
            p.b.clone(),
            &sent.frame,
            &expected(&inner),
            Some(&p.trust_a),
            observed,
            1000
        )
        .is_err());
    }
}
#[test]
fn fresh_trust_and_exact_selected_bundle_account_profile_are_required() {
    let p = pair();
    let end = p.a.remote.selected.publisher_peer;
    let pending = ContactTrust::pending(p.trust_b.candidate().clone());
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
    assert_eq!(changed.state(), kami_identity::ContactTrustState::Changed);
    for trust in [
        None,
        Some(&pending),
        Some(&blocked),
        Some(&newer),
        Some(&changed),
        Some(&p.trust_a),
    ] {
        assert!(BoundSession::outbound(&p.alice, p.a.clone(), trust, Some(&end), 1000).is_err());
    }
    for n in 0..12 {
        let mut a = p.a.clone();
        match n {
            0 => a.remote.selected.binding_generation += 1,
            1 => a.remote.selected.bundle_generation += 1,
            2 => a.remote.selected.prekey_reference.push(3),
            3 => a.remote.selected.prekey_kind = OlmPrekeyKind::Fallback,
            4 => a.remote.selected.prekey_curve25519[0] ^= 1,
            5 => a.remote.selected.olm_curve25519[0] ^= 1,
            6 => a.remote.selected.olm_ed25519[0] ^= 1,
            7 => a.remote.device = a.local.device,
            8 => a.remote.selected.publisher_peer = a.local.selected.publisher_peer,
            9 => a.trust_generation += 1,
            10 => a.local.selected.olm_curve25519[0] ^= 1,
            11 => a.local.selected.prekey_reference.push(3),
            _ => unreachable!(),
        }
        assert!(
            BoundSession::outbound(&p.alice, a, Some(&p.trust_b), Some(&end), 1000).is_err(),
            "selection {n}"
        );
    }
    for now in [0, 2000, u64::MAX] {
        assert!(
            BoundSession::outbound(&p.alice, p.a.clone(), Some(&p.trust_b), Some(&end), now)
                .is_err()
        );
    }
    assert!(
        BoundSession::outbound(&p.bob, p.a.clone(), Some(&p.trust_b), Some(&end), 1000).is_err()
    );
}
#[test]
fn local_trust_generation_is_checked_per_operation_not_shared_in_the_envelope() {
    let mut p = pair();
    let stale = p.trust_b.clone();
    p.trust_b = p
        .trust_b
        .transition(TrustTransition::Observe(p.trust_b.candidate().clone()))
        .unwrap();
    p.a.trust_generation = p.trust_b.generation();
    assert_ne!(p.a.trust_generation, p.b.trust_generation);
    let a = outbound(&p);
    let inner = inner_for(&p, &a);
    assert!(a
        .stage_send(&inner, Some(&stale), Some(&inner.recipient_peer), 1000)
        .is_err());
    let sent = send(&p, &a, &inner);
    let blocked = p.trust_a.transition(TrustTransition::Block).unwrap();
    assert!(BoundSession::stage_inbound(
        &p.bob,
        p.b.clone(),
        &sent.frame,
        &expected(&inner),
        Some(&blocked),
        Some(&inner.sender_peer),
        1000
    )
    .is_err());
    let b = inbound(&p, &sent.frame, &inner).unwrap();
    let mut second = inner.clone();
    second.context.message = kami_core::MessageId::from_u128(99);
    let sent2 = send(&p, &sent.session, &second);
    assert!(b
        .session
        .stage_receive(
            &sent2.frame,
            &expected(&second),
            Some(&blocked),
            Some(&inner.sender_peer),
            1000
        )
        .is_err());
    assert!(b
        .session
        .stage_receive(
            &sent2.frame,
            &expected(&second),
            Some(&p.trust_a),
            Some(&inner.sender_peer),
            1000
        )
        .is_ok());
}

#[test]
fn forged_binding_and_noncontributory_signed_prekey_cannot_establish_session() {
    let p = pair();
    let mut a = p.a.clone();
    let mut bytes = a.remote.binding.to_bytes();
    *bytes.last_mut().unwrap() ^= 1;
    a.remote.binding = OlmBindingProof::parse_bounded(&bytes).unwrap();
    assert!(BoundSession::outbound(
        &p.alice,
        a,
        Some(&p.trust_b),
        Some(&p.a.remote.selected.publisher_peer),
        1000
    )
    .is_err());
    let mut a = p.a.clone();
    a.remote.peer_binding = p.a.local.peer_binding.clone();
    assert!(BoundSession::outbound(
        &p.alice,
        a,
        Some(&p.trust_b),
        Some(&p.a.remote.selected.publisher_peer),
        1000
    )
    .is_err());
    let mut a = p.a.clone();
    a.remote.selected.prekey_curve25519 = [0; 32];
    a.remote.binding = sign_selection(&a.remote.selected, 8);
    assert!(BoundSession::outbound(
        &p.alice,
        a,
        Some(&p.trust_b),
        Some(&p.a.remote.selected.publisher_peer),
        1000
    )
    .is_err());
}

#[test]
fn reply_receipt_and_committed_session_replay_rejection() {
    let p = pair();
    let a = outbound(&p);
    let text = inner_for(&p, &a);
    let sent = send(&p, &a, &text);
    let b = inbound(&p, &sent.frame, &text).unwrap();
    assert!(b
        .session
        .stage_receive(
            &sent.frame,
            &expected(&text),
            Some(&p.trust_a),
            Some(&text.sender_peer),
            1000
        )
        .is_err());
    let mut receipt = text.clone();
    std::mem::swap(&mut receipt.context.sender, &mut receipt.context.recipient);
    std::mem::swap(&mut receipt.sender_peer, &mut receipt.recipient_peer);
    std::mem::swap(&mut receipt.sender_binding, &mut receipt.recipient_binding);
    receipt.direction = Direction::ResponderToInitiator;
    receipt.kind = kami_protocol::MessageKind::DeviceDelivered;
    receipt.context.message = kami_core::MessageId::from_u128(3);
    receipt.receipt_target = Some(text.context.message);
    receipt.sequence = None;
    receipt.payload.clear();
    let reply = b
        .session
        .stage_send(
            &receipt,
            Some(&p.trust_a),
            Some(&receipt.recipient_peer),
            1000,
        )
        .unwrap();
    let received = sent
        .session
        .stage_receive(
            &reply.frame,
            &expected(&receipt),
            Some(&p.trust_b),
            Some(&receipt.sender_peer),
            1000,
        )
        .unwrap();
    assert_eq!(received.inner, receipt);
    let decoded = kami_protocol::decode_frame(&reply.frame).unwrap();
    let mut substituted = receipt.clone();
    substituted.receipt_target = Some(kami_core::MessageId::from_u128(99));
    let forged = outer(
        &substituted,
        decoded.protected_payload(),
        decoded.authentication_context(),
    );
    assert!(sent
        .session
        .stage_receive(
            &forged,
            &expected(&substituted),
            Some(&p.trust_b),
            Some(&receipt.sender_peer),
            1000
        )
        .is_err());
    assert!(received
        .session
        .stage_receive(
            &reply.frame,
            &expected(&receipt),
            Some(&p.trust_b),
            Some(&receipt.sender_peer),
            1000
        )
        .is_err());
    let mut wrong = expected(&receipt);
    wrong.receipt_target = Some(kami_core::MessageId::from_u128(99));
    assert!(sent
        .session
        .stage_receive(
            &reply.frame,
            &wrong,
            Some(&p.trust_b),
            Some(&receipt.sender_peer),
            1000
        )
        .is_err());
    // Failure must not advance source; correct retry remains acceptable.
    assert!(sent
        .session
        .stage_receive(
            &reply.frame,
            &expected(&receipt),
            Some(&p.trust_b),
            Some(&receipt.sender_peer),
            1000
        )
        .is_ok());
}
