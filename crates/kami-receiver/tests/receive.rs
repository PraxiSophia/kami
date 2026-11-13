#[path = "../../kami-storage/tests/common/mod.rs"]
mod common;
mod failures;
use kami_core::{ConversationId, DeviceId, MessageId};
use kami_e2ee::{Endpoint, QuarantinedSession};
use kami_identity::{
    ContactTrust, Invitation, OlmBindingData, OlmBindingProof, OlmPrekeyKind, PeerBindingProof,
    TrustTransition, VerificationEvidence,
};
use kami_receiver::{ReceiveContext, ReceiveOutcome, Receiver};
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
fn seed(s: &kami_storage::Storage, t: &ContactTrust, id: DeviceId) {
    s.compare_and_swap_contact(
        None,
        ContactTrustRow::new(
            *id.as_bytes(),
            TrustState::Pending,
            1,
            None,
            t.candidate().to_bytes(),
        )
        .unwrap(),
    )
    .unwrap();
    s.compare_and_swap_contact(
        Some(1),
        ContactTrustRow::new(
            *id.as_bytes(),
            TrustState::Verified,
            t.generation(),
            Some(t.pinned().unwrap().to_bytes()),
            t.candidate().to_bytes(),
        )
        .unwrap(),
    )
    .unwrap();
}
#[test]
fn coordinator_persists_before_ack_and_recovers_without_live_ratchet() {
    let Some(_native) = common::native::isolated() else {
        return;
    };
    let mut alice = Account::new();
    alice.generate_one_time_keys(1);
    let mut bob = Account::new();
    bob.generate_one_time_keys(1);
    let (a, ta, aid) = endpoint(&alice, 7);
    let (b, tb, bid) = endpoint(&bob, 8);
    let ap = ta.pinned().unwrap().binding().peer_id();
    let bp = tb.pinned().unwrap().binding().peer_id();
    let ad = common::directory();
    let bd = common::directory();
    let sa = common::fresh_create(&ad);
    let sb = common::fresh_create(&bd);
    seed(&sa, &tb, bid);
    seed(&sb, &ta, aid);
    let source = QuarantinedSession::outbound(
        alice,
        a.clone(),
        b.clone(),
        ConversationId::from_u128(1),
        &tb,
        &bp,
        1000,
    )
    .unwrap();
    let request = kami_sender::SendRequest::new(
        MessageId::from_u128(1),
        ConversationId::from_u128(1),
        aid,
        bid,
        1000,
        "private receiver text".into(),
    )
    .unwrap();
    let kami_sender::EnqueueOutcome::Committed {
        message,
        proposed_session: sender,
    } = kami_sender::Coordinator::new(&sa)
        .enqueue(request, &source, &tb, &bp, 1000)
        .unwrap()
    else {
        panic!()
    };
    let frame = message.frame();
    let ctx =
        || ReceiveContext::new(bid, aid, ConversationId::from_u128(1), &ta, &ap, 1000).unwrap();
    assert!(Receiver::new(&sb)
        .receive(&vec![0; 65537], ctx(), None, MessageId::from_u128(2))
        .is_err());
    let ReceiveOutcome::New {
        delivery,
        proposed_session: receiver,
    } = Receiver::new(&sb)
        .receive_first(frame, ctx(), bob, b, a, MessageId::from_u128(2))
        .unwrap()
    else {
        panic!()
    };
    let ack = delivery.receipt().to_vec();
    assert_eq!(
        sb.history(message.facts().conversation_id(), 10).unwrap()[0].payload(),
        b"private receiver text"
    );
    let ReceiveOutcome::Duplicate { delivery } = Receiver::new(&sb)
        .receive(frame, ctx(), Some(&receiver), MessageId::from_u128(99))
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(delivery.receipt(), ack);
    sb.shutdown().unwrap();
    let sb = common::open(&bd);
    let ReceiveOutcome::Duplicate { delivery } = Receiver::new(&sb)
        .receive(frame, ctx(), None, MessageId::from_u128(99))
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(delivery.receipt(), ack);
    let outer = kami_protocol::decode_frame(&ack).unwrap();
    for mutation in 0..6 {
        let mut payload = outer.protected_payload().to_vec();
        let mut authentication = outer.authentication_context().to_vec();
        if mutation == 4 {
            authentication[0] ^= 1;
        }
        if mutation == 5 {
            let last = payload.len() - 1;
            payload[last] ^= 1;
        }
        let forged = kami_protocol::encode_frame(
            &kami_protocol::StructurallyValidFrame::new(kami_protocol::FrameInput {
                message_id: if mutation == 0 {
                    MessageId::from_u128(98)
                } else {
                    outer.message_id()
                },
                conversation_id: if mutation == 1 {
                    ConversationId::from_u128(98)
                } else {
                    outer.conversation_id()
                },
                sender: if mutation == 2 {
                    DeviceId::from_bytes([99; 32])
                } else {
                    outer.sender()
                },
                recipient: outer.recipient(),
                kind: outer.kind(),
                sequence: None,
                receipt_target: if mutation == 3 {
                    Some(MessageId::from_u128(98))
                } else {
                    outer.receipt_target()
                },
                created_at_ms: outer.created_at_ms(),
                protected_payload: &payload,
                authentication_context: &authentication,
            })
            .unwrap(),
        )
        .unwrap();
        assert!(
            kami_sender::Coordinator::new(&sa)
                .receive_receipt(&forged, Some(&sender), &bp, 1000)
                .is_err(),
            "mutation {mutation}"
        );
        assert_eq!(
            sa.sender_record(aid, MessageId::from_u128(1))
                .unwrap()
                .state(),
            kami_storage::SendState::Queued
        );
    }
    assert!(kami_sender::Coordinator::new(&sa)
        .receive_receipt(&ack, Some(&sender), &ap, 1000)
        .is_err());
    // Cancel stops retries; it cannot invalidate a genuine later delivery receipt.
    sa.cancel_send(aid, MessageId::from_u128(1)).unwrap();
    let outcome = kami_sender::Coordinator::new(&sa)
        .receive_receipt(&ack, Some(&sender), &bp, 1000)
        .unwrap();
    assert!(matches!(
        outcome,
        kami_sender::ReceiptOutcome::Delivered { .. }
    ));
    assert!(matches!(
        kami_sender::Coordinator::new(&sa)
            .receive_receipt(&ack, None, &bp, 1000)
            .unwrap(),
        kami_sender::ReceiptOutcome::Duplicate(_)
    ));
    assert_eq!(
        sa.sender_record(aid, MessageId::from_u128(1))
            .unwrap()
            .state(),
        kami_storage::SendState::Delivered
    );
    sa.shutdown().unwrap();
    sb.shutdown().unwrap();
}
