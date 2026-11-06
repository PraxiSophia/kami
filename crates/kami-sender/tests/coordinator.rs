#[path = "../../kami-storage/tests/common/mod.rs"]
mod common;
use kami_core::{ConversationId, DeviceId, MessageId};
use kami_e2ee::{Endpoint, QuarantinedSession};
use kami_identity::{
    ContactTrust, Invitation, OlmBindingData, OlmBindingProof, OlmPrekeyKind, PeerBindingProof,
    TrustTransition, VerificationEvidence,
};
use kami_sender::{Coordinator, EnqueueOutcome, SendRequest};
use kami_storage::{ContactTrustRow, TrustState};
use libp2p_identity::Keypair;
use vodozemac::olm::Account;

fn endpoint(account: &Account, seed: u8) -> (Endpoint, ContactTrust, DeviceId) {
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
    let binding = PeerBindingProof::parse_bounded(&wire).unwrap();
    let pending = ContactTrust::pending(Invitation::new(public, binding.clone()).unwrap());
    let evidence = VerificationEvidence::independent_comparison(
        &pending,
        pending.candidate().fingerprint(),
        true,
    )
    .unwrap();
    let trust = pending
        .transition(TrustTransition::Verify(evidence))
        .unwrap();
    let selected = OlmBindingData {
        publisher_peer: peer,
        olm_ed25519: *account.ed25519_key().as_bytes(),
        olm_curve25519: *account.curve25519_key().as_bytes(),
        prekey_kind: OlmPrekeyKind::OneTime,
        prekey_reference: vec![seed],
        prekey_curve25519: *account.one_time_keys().values().next().unwrap().as_bytes(),
        binding_generation: 1,
        bundle_generation: 1,
        not_before_ms: 1,
        expires_at_ms: 2000,
    };
    let mut wire = b"kami-olm-binding-v1\0".to_vec();
    wire.push(1);
    wire.extend(public);
    let bytes = peer.to_bytes();
    wire.extend((bytes.len() as u16).to_be_bytes());
    wire.extend(bytes);
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
    let id = DeviceId::from_bytes(public);
    (
        Endpoint::new(
            id,
            binding,
            OlmBindingProof::parse_bounded(&wire).unwrap(),
            selected,
        ),
        trust,
        id,
    )
}
#[test]
fn enqueue_and_restart_resubmission_do_not_encrypt_twice() {
    let mut alice = Account::new();
    alice.generate_one_time_keys(1);
    let mut bob = Account::new();
    bob.generate_one_time_keys(1);
    let (a, _, aid) = endpoint(&alice, 7);
    let (b, trust, bid) = endpoint(&bob, 8);
    let peer = trust.pinned().unwrap().binding().peer_id();
    let fork = QuarantinedSession::outbound(
        Account::from_pickle(alice.pickle()),
        a.clone(),
        b.clone(),
        ConversationId::from_u128(1),
        &trust,
        &peer,
        1000,
    )
    .unwrap();
    let source = QuarantinedSession::outbound(
        alice,
        a,
        b,
        ConversationId::from_u128(1),
        &trust,
        &peer,
        1000,
    )
    .unwrap();
    let dir = common::directory();
    let storage = common::create(&dir);
    storage
        .compare_and_swap_contact(
            None,
            ContactTrustRow::new(
                *bid.as_bytes(),
                TrustState::Pending,
                1,
                None,
                trust.candidate().to_bytes(),
            )
            .unwrap(),
        )
        .unwrap();
    storage
        .compare_and_swap_contact(
            Some(1),
            ContactTrustRow::new(
                *bid.as_bytes(),
                TrustState::Verified,
                2,
                Some(trust.pinned().unwrap().to_bytes()),
                trust.candidate().to_bytes(),
            )
            .unwrap(),
        )
        .unwrap();
    let request = SendRequest::new(
        MessageId::from_u128(33),
        ConversationId::from_u128(1),
        aid,
        bid,
        1000,
        "private sender text".into(),
    )
    .unwrap();
    // Inject failure at the final v4 insert: all crypto/message changes must roll back.
    common::raw(&dir).execute_batch("CREATE TRIGGER sender_fault BEFORE INSERT ON sender_attempt BEGIN SELECT RAISE(ABORT,'injected'); END").unwrap();
    assert!(matches!(
        Coordinator::new(&storage).enqueue(request.clone(), &source, &trust, &peer, 1000),
        Err(kami_sender::Error::RolledBack(_))
    ));
    assert_eq!(
        common::raw(&dir)
            .query_row("SELECT count(*) FROM e2ee_frame", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    common::raw(&dir)
        .execute_batch("DROP TRIGGER sender_fault")
        .unwrap();
    // Exercise concurrent same-ID preparation without allowing two proposals to install.
    let barrier = std::sync::Barrier::new(8);
    let committed = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let storage = &storage;
                let request = request.clone();
                let source = if i % 2 == 0 { &source } else { &fork };
                let trust = &trust;
                let peer = &peer;
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    Coordinator::new(storage).enqueue(request, source, trust, peer, 1000)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .filter(|r| {
                if let Ok(EnqueueOutcome::Committed {
                    message,
                    proposed_session,
                }) = r
                {
                    let material = proposed_session
                        .prepare_retry(&trust, &peer, 1000)
                        .unwrap()
                        .into_material();
                    assert_eq!(
                        material.frame(),
                        message.frame(),
                        "never return a losing ratchet proposal"
                    );
                    true
                } else {
                    false
                }
            })
            .count()
    });
    assert!(committed > 0);
    let EnqueueOutcome::Recovered(message) = Coordinator::new(&storage)
        .enqueue(request.clone(), &source, &trust, &peer, 1000)
        .unwrap()
    else {
        panic!()
    };
    let frame = message.frame().to_vec();
    storage.shutdown().unwrap();
    let storage = common::open(&dir);
    // The stale source would fork the ratchet if prepare_text were called again.
    let EnqueueOutcome::Recovered(recovered) = Coordinator::new(&storage)
        .enqueue(request, &source, &trust, &peer, 1001)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(recovered.frame(), frame);
    assert_eq!(recovered.attempts(), 0);
    let wrong_peer = Keypair::ed25519_from_bytes([99; 32])
        .unwrap()
        .public()
        .to_peer_id();
    assert!(matches!(
        Coordinator::new(&storage).claim(aid, MessageId::from_u128(33), &wrong_peer, 1001),
        Err(kami_sender::Error::IdentityHeld)
    ));
    Coordinator::new(&storage)
        .resume(aid, MessageId::from_u128(33), &peer, 1002)
        .unwrap();
    let lease = Coordinator::new(&storage)
        .claim(aid, MessageId::from_u128(33), &peer, 1003)
        .unwrap()
        .unwrap();
    assert_eq!(lease.message().attempts(), 1);
    assert_eq!(lease.message().frame(), frame);
    assert_eq!(
        common::raw(&dir)
            .query_row("SELECT generation FROM e2ee_session", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    storage.shutdown().unwrap();
}
