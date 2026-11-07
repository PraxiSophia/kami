use crate::{
    ingress::{AuthenticatedPeer, Operation},
    tests::{directory, profile},
    Config, Engine, ShutdownOutcome,
};
use kami_core::{ConversationId, DeviceId, MessageId};
use kami_e2ee::{Endpoint, QuarantinedSession};
use kami_identity::{
    ContactTrust, Invitation, OlmBindingData, OlmBindingProof, OlmPrekeyKind, PeerBindingProof,
    TrustTransition, VerificationEvidence,
};
use kami_storage::{ContactTrustRow, Storage, TrustState};
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
    (
        Endpoint::new(
            DeviceId::from_bytes(public),
            binding,
            OlmBindingProof::parse_bounded(&wire).unwrap(),
            selected,
        ),
        trust,
        DeviceId::from_bytes(public),
    )
}
fn seed(storage: &Storage, trust: &ContactTrust, id: DeviceId) {
    storage
        .compare_and_swap_contact(
            None,
            ContactTrustRow::new(
                *id.as_bytes(),
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
                *id.as_bytes(),
                TrustState::Verified,
                trust.generation(),
                Some(trust.pinned().unwrap().to_bytes()),
                trust.candidate().to_bytes(),
            )
            .unwrap(),
        )
        .unwrap();
}
#[tokio::test]
async fn lost_enqueue_reply_reconciles_exact_frame_after_runtime_restart() {
    let dir = directory();
    let mut reopen = profile(&dir, None);
    reopen.create = false;
    let (engine, _) = Engine::start(profile(&dir, None), Config::default()).unwrap();
    engine.wait_running().await.unwrap();
    let (committed, durable) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let e = engine.clone();
    let caller = tokio::spawn(async move {
        e.test_work(move |storage| {
            let mut alice = Account::new();
            alice.generate_one_time_keys(1);
            let mut bob = Account::new();
            bob.generate_one_time_keys(1);
            let (a, _, aid) = endpoint(&alice, 7);
            let (b, tb, bid) = endpoint(&bob, 8);
            seed(storage, &tb, bid);
            let c = ConversationId::from_u128(1);
            let id = MessageId::from_u128(17);
            let source = QuarantinedSession::outbound(
                alice,
                a,
                b,
                c,
                &tb,
                &tb.pinned().unwrap().binding().peer_id(),
                1000,
            )
            .unwrap();
            let request =
                kami_sender::SendRequest::new(id, c, aid, bid, 1000, "lost response".into())
                    .unwrap();
            let peer = AuthenticatedPeer::mock(aid, bid, c, tb, Operation::Enqueue(id));
            let kami_sender::EnqueueOutcome::Committed { message, .. } =
                peer.enqueue(storage, request, &source, 1000)?
            else {
                panic!()
            };
            committed.send((aid, id, message.frame().to_vec())).unwrap();
            wait.recv().unwrap();
            Ok(1)
        })
        .await
    });
    let (sender, id, frame) = durable.await.unwrap();
    caller.abort(); // durability succeeded but caller loses its success response
    release.send(()).unwrap();
    assert_eq!(engine.shutdown().await, ShutdownOutcome::Clean);
    engine.wait_stopped().await;
    let (again, _) = Engine::start(reopen, Config::default()).unwrap();
    again.wait_running().await.unwrap();
    assert_eq!(again.snapshot().recovered, 1);
    assert_eq!(
        again
            .test_work(move |storage| {
                let stored = storage.sender_record(sender, id).unwrap();
                assert_eq!(stored.frame(), frame);
                assert_eq!(stored.state(), kami_storage::SendState::Queued);
                assert_eq!(stored.attempts(), 0);
                Ok(1)
            })
            .await,
        Ok(1)
    );
    assert_eq!(again.shutdown().await, ShutdownOutcome::Clean);
}

#[tokio::test]
async fn private_mock_ingress_reuses_sender_receiver_and_durable_receipts_on_blocking_owner() {
    let ad = directory();
    let bd = directory();
    let bob_profile = profile(&bd, None);
    let (engine, _) = Engine::start(profile(&ad, None), Config::default()).unwrap();
    engine.wait_running().await.unwrap();
    let result = engine
        .test_work(move |sa| {
            let sb = Storage::create(
                kami_storage::ProfileLease::acquire(&bob_profile.directory).unwrap(),
                bob_profile.key_id,
                bob_profile.key_provider,
                bob_profile.storage,
            )
            .unwrap();
            let mut alice = Account::new();
            alice.generate_one_time_keys(1);
            let mut bob = Account::new();
            bob.generate_one_time_keys(1);
            let (a, ta, aid) = endpoint(&alice, 7);
            let (b, tb, bid) = endpoint(&bob, 8);
            seed(sa, &tb, bid);
            seed(&sb, &ta, aid);
            let conversation = ConversationId::from_u128(1);
            let mid = MessageId::from_u128(1);
            let rid = MessageId::from_u128(2);
            let bp = tb.pinned().unwrap().binding().peer_id();
            let ap = ta.pinned().unwrap().binding().peer_id();
            let source = QuarantinedSession::outbound(
                alice,
                a.clone(),
                b.clone(),
                conversation,
                &tb,
                &bp,
                1000,
            )
            .unwrap();
            let request = || {
                kami_sender::SendRequest::new(
                    mid,
                    conversation,
                    aid,
                    bid,
                    1000,
                    "private runtime integration".into(),
                )
                .unwrap()
            };
            let evidence = || {
                AuthenticatedPeer::mock(aid, bid, conversation, tb.clone(), Operation::Enqueue(mid))
            };
            for fault in 0..6 {
                let mut e = evidence();
                match fault {
                    0 => e.disconnect(),
                    1 => e.change_epoch(),
                    2 => e.substitute_peer(ap), // direct wrong peer / relay substituted as end peer
                    3 => e.substitute_peer(
                        Keypair::ed25519_from_bytes([81; 32])
                            .unwrap()
                            .public()
                            .to_peer_id(),
                    ),
                    _ => {}
                }
                let now = if fault == 4 { 2000 } else { 1000 };
                let mut r = request();
                if fault == 5 {
                    r.message = MessageId::from_u128(99);
                }
                assert!(e.enqueue(sa, r, &source, now).is_err());
            }
            assert!(sa.recover_sends(10).unwrap().is_empty());
            let kami_sender::EnqueueOutcome::Committed {
                message,
                proposed_session: sender,
            } = evidence().enqueue(sa, request(), &source, 1000)?
            else {
                panic!()
            };
            // Same-ID request and lost client reply recover exact durable bytes, no ratchet.
            let kami_sender::EnqueueOutcome::Recovered(recovered) =
                evidence().enqueue(sa, request(), &source, 1000)?
            else {
                panic!()
            };
            assert_eq!(recovered.frame(), message.frame());
            let claim =
                AuthenticatedPeer::mock(aid, bid, conversation, tb.clone(), Operation::Claim(mid));
            assert_eq!(crate::worker::retry_due(sa, 999).unwrap(), 1);
            assert_eq!(crate::worker::retry_due(sa, 1000).unwrap(), 1);
            let lease = claim.claim(sa, mid, 1000)?.unwrap();
            assert_eq!(crate::worker::retry_due(sa, 1000).unwrap(), 0);
            assert_eq!(
                crate::worker::retry_due(sa, lease.expires_at_ms()).unwrap(),
                1
            );
            assert_eq!(lease.message().frame(), message.frame());
            assert_eq!(
                sa.sender_record(aid, mid).unwrap().state(),
                kami_storage::SendState::InFlight
            );
            // Mock transport write changes nothing: no DeviceDelivered state.
            assert_ne!(
                sa.sender_record(aid, mid).unwrap().state(),
                kami_storage::SendState::Delivered
            );
            let ctx =
                kami_receiver::ReceiveContext::new(bid, aid, conversation, &ta, &ap, 1000).unwrap();
            let kami_receiver::ReceiveOutcome::New {
                delivery,
                proposed_session: receiver,
            } = kami_receiver::Receiver::new(&sb)
                .receive_first(message.frame(), ctx, bob, b, a, rid)
                .unwrap()
            else {
                panic!()
            };
            assert_eq!(
                sb.history(conversation, 10).unwrap()[0].payload(),
                b"private runtime integration"
            );
            let ack = delivery.receipt().to_vec();
            let receive = AuthenticatedPeer::mock(
                bid,
                aid,
                conversation,
                ta.clone(),
                Operation::Receive(mid),
            );
            let kami_receiver::ReceiveOutcome::Duplicate { delivery } = receive.receive(
                &sb,
                message.frame(),
                Some(&receiver),
                MessageId::from_u128(99),
                1000,
            )?
            else {
                panic!()
            };
            assert_eq!(delivery.receipt(), ack);
            assert_eq!(sb.history(conversation, 10).unwrap().len(), 1);
            let receipt = AuthenticatedPeer::mock(
                aid,
                bid,
                conversation,
                tb.clone(),
                Operation::Receipt(rid),
            );
            let mut forged = ack.clone();
            let last = forged.len() - 1;
            forged[last] ^= 1;
            assert!(receipt.receipt(sa, &forged, Some(&sender), 1000).is_err());
            assert_ne!(
                sa.sender_record(aid, mid).unwrap().state(),
                kami_storage::SendState::Delivered
            );
            assert!(matches!(
                receipt.receipt(sa, &ack, Some(&sender), 1000)?,
                kami_sender::ReceiptOutcome::Delivered { .. }
            ));
            assert!(matches!(
                receipt.receipt(sa, &ack, None, 1000)?,
                kami_sender::ReceiptOutcome::Duplicate(_)
            ));
            assert_eq!(
                sa.sender_record(aid, mid).unwrap().state(),
                kami_storage::SendState::Delivered
            );
            // Fresh durable pin revocation defeats an already-created mock capability.
            sa.compare_and_swap_contact(
                Some(tb.generation()),
                ContactTrustRow::new(
                    *bid.as_bytes(),
                    TrustState::Blocked,
                    tb.generation() + 1,
                    Some(tb.pinned().unwrap().to_bytes()),
                    tb.candidate().to_bytes(),
                )
                .unwrap(),
            )
            .unwrap();
            assert!(receipt.receipt(sa, &ack, None, 1000).is_err());
            sb.shutdown().unwrap();
            Ok(1)
        })
        .await;
    assert_eq!(result, Ok(1));
    assert_eq!(engine.shutdown().await, ShutdownOutcome::Clean);
    engine.wait_stopped().await;
}
