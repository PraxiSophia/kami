use super::*;
use crate::authenticated::QuarantinedSession;
use kami_core::{MessageId, SenderSequence};

#[test]
fn inbound_delivery_stages_both_directions_and_binds_the_exact_target() {
    let p = pair();
    let a = QuarantinedSession::outbound(
        Account::from_pickle(p.alice.pickle()),
        p.a.local.clone(),
        p.a.remote.clone(),
        p.a.conversation,
        &p.trust_b,
        &p.a.remote.selected.publisher_peer,
        1000,
    )
    .unwrap();
    let (sent, a) = a
        .prepare_text(
            MessageId::from_u128(11),
            SenderSequence::new(1),
            1000,
            "authenticated text",
            &p.trust_b,
            &p.a.remote.selected.publisher_peer,
            1000,
        )
        .unwrap();
    let facts = sent.facts();
    let frame = sent.into_material().frame().to_vec();
    let expected = kami_protocol::ExpectedContext {
        message_id: facts.message_id(),
        conversation_id: facts.conversation_id(),
        sender: facts.sender(),
        recipient: facts.recipient(),
        receipt_target: None,
    };
    let (delivery, b) = QuarantinedSession::inbound_delivery(
        Account::from_pickle(p.bob.pickle()),
        p.b.local.clone(),
        p.b.remote.clone(),
        p.b.conversation,
        &frame,
        &expected,
        MessageId::from_u128(12),
        &p.trust_a,
        &p.b.remote.selected.publisher_peer,
        1000,
    )
    .unwrap();
    assert_eq!(delivery.facts(), facts);
    let receipt = delivery.receipt_frame().to_vec();
    let outer = kami_protocol::decode_frame(&receipt).unwrap();
    assert_eq!(outer.kind(), kami_protocol::MessageKind::DeviceDelivered);
    assert_eq!(outer.sequence(), None);
    assert_eq!(outer.receipt_target(), Some(facts.message_id()));
    let expected_receipt = kami_protocol::ExpectedContext {
        message_id: MessageId::from_u128(12),
        sender: facts.recipient(),
        recipient: facts.sender(),
        receipt_target: Some(facts.message_id()),
        ..expected
    };
    let wrong = kami_core::MessageFacts::new(
        facts.message_id(),
        facts.conversation_id(),
        facts.sender(),
        facts.recipient(),
        facts.sequence(),
        kami_core::ContentBinding::from_bytes([99; 32]),
    );
    assert!(a
        .prepare_receipt(
            &receipt,
            &expected_receipt,
            wrong,
            &frame,
            &p.trust_b,
            &p.a.remote.selected.publisher_peer,
            1000
        )
        .is_err());
    let (accepted, advanced) = a
        .prepare_receipt(
            &receipt,
            &expected_receipt,
            facts,
            &frame,
            &p.trust_b,
            &p.a.remote.selected.publisher_peer,
            1000,
        )
        .unwrap();
    assert_eq!(accepted.facts(), facts);
    assert!(advanced
        .prepare_receipt(
            &receipt,
            &expected_receipt,
            facts,
            &frame,
            &p.trust_b,
            &p.a.remote.selected.publisher_peer,
            1000
        )
        .is_err());
    assert!(b
        .prepare_receive(
            &frame,
            &expected,
            &p.trust_a,
            &p.b.remote.selected.publisher_peer,
            1000
        )
        .is_err());
    let (next, _) = b
        .prepare_text(
            MessageId::from_u128(13),
            SenderSequence::new(1),
            1000,
            "reply after receipt",
            &p.trust_a,
            &p.b.remote.selected.publisher_peer,
            1000,
        )
        .unwrap();
    let next_frame = next.into_material().frame().to_vec();
    let next_expected = kami_protocol::ExpectedContext {
        message_id: MessageId::from_u128(13),
        receipt_target: None,
        ..expected_receipt
    };
    assert!(advanced
        .prepare_receive(
            &next_frame,
            &next_expected,
            &p.trust_b,
            &p.a.remote.selected.publisher_peer,
            1000
        )
        .is_ok());
}
