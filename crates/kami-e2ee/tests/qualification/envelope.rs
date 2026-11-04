use crate::envelope::{Direction, InnerEnvelopeV1};
use kami_core::{ConversationId, DeviceId, MessageId, SenderSequence};
use kami_protocol::MessageKind;
use libp2p_identity::Keypair;

pub(super) fn sample() -> InnerEnvelopeV1 {
    let peer = Keypair::ed25519_from_bytes([8; 32])
        .unwrap()
        .public()
        .to_peer_id();
    InnerEnvelopeV1 {
        context: crate::ApplicationContext {
            conversation: ConversationId::from_u128(1),
            message: MessageId::from_u128(2),
            sender: DeviceId::from_bytes([3; 32]),
            recipient: DeviceId::from_bytes([4; 32]),
        },
        kind: MessageKind::Text,
        sequence: Some(SenderSequence::new(1)),
        receipt_target: None,
        created_at_ms: 1000,
        session_id: "A".repeat(43),
        direction: Direction::InitiatorToResponder,
        sender_peer: peer,
        recipient_peer: peer,
        sender_binding: [5; 32],
        recipient_binding: [6; 32],
        payload: b"private fixture".to_vec(),
    }
}

#[test]
fn canonical_inner_roundtrip_and_commitment() {
    let inner = sample();
    let bytes = inner.encode().unwrap();
    assert_eq!(InnerEnvelopeV1::decode(&bytes).unwrap(), inner);
    use sha2::{Digest, Sha256};
    let mut independent = Sha256::new();
    independent.update(b"KAMI-INNER-CONTEXT-V1\0");
    independent.update(&bytes);
    assert_eq!(
        inner.commitment().unwrap().as_slice(),
        independent.finalize().as_slice()
    );
    assert!(!format!("{inner:?}").contains("private fixture"));
}

#[test]
fn inner_rejects_out_of_protocol_timestamp_range() {
    let mut inner = sample();
    inner.created_at_ms = 253_402_300_800_000;
    assert!(inner.encode().is_err());
}

#[test]
fn rejects_every_truncation_trailing_duplicate_unknown_and_noncanonical_inner_encoding() {
    let bytes = sample().encode().unwrap();
    for n in 0..bytes.len() {
        assert!(
            InnerEnvelopeV1::decode(&bytes[..n]).is_err(),
            "truncation {n}"
        );
    }
    for extra in [vec![0], bytes.clone(), vec![0x80, 1]] {
        let mut b = bytes.clone();
        b.extend(extra);
        assert!(InnerEnvelopeV1::decode(&b).is_err());
    }
    assert!(InnerEnvelopeV1::decode(&vec![0; 65_537]).is_err());
    let base = b"KAMI-INNER-V1\0".len();
    for (offset, value) in [
        (base, 2),
        (base + 1, 1),
        (base + 2, 2),
        (base + 3, 3),
        (base + 4, 0),
    ] {
        let mut b = bytes.clone();
        b[offset] = value;
        assert!(InnerEnvelopeV1::decode(&b).is_err(), "offset {offset}");
    }
    let mut b = bytes.clone();
    *b.last_mut().unwrap() = 0xff;
    assert!(InnerEnvelopeV1::decode(&b).is_err());
    // Hostile length prefix, checked before payload allocation.
    let mut b = bytes.clone();
    let start = b.len() - sample().payload.len() - 4;
    b[start..start + 4].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(InnerEnvelopeV1::decode(&b).is_err());
    // Noncanonical session-ID base64's unused bits.
    let mut inner = sample();
    inner.session_id = "B".repeat(43);
    assert!(inner.encode().is_err());
    // Nonminimal multihash varint: canonical PeerId must round-trip exactly.
    let start = base + 5 + 16 + 16 + 32 + 32 + 43 + 8 + 8;
    let mut b = bytes.clone();
    let len = u16::from_be_bytes([b[start], b[start + 1]]);
    b[start..start + 2].copy_from_slice(&(len + 1).to_be_bytes());
    b[start + 2] |= 0x80;
    b.insert(start + 3, 0);
    assert!(InnerEnvelopeV1::decode(&b).is_err());
}
#[test]
fn message_kind_invariants_and_utf8_fail_closed() {
    for n in 0..8 {
        let mut i = sample();
        match n {
            0 => i.payload = vec![0xff],
            1 => i.payload.clear(),
            2 => i.sequence = None,
            3 => i.sequence = Some(SenderSequence::new(0)),
            4 => i.receipt_target = Some(MessageId::from_u128(5)),
            5 => i.context.message = MessageId::from_u128(0),
            6 => i.payload = vec![b'x'; 65_536],
            7 => i.context.recipient = i.context.sender,
            _ => unreachable!(),
        }
        assert!(i.encode().is_err(), "invariant {n}");
    }
    let mut receipt = sample();
    receipt.kind = MessageKind::DeviceDelivered;
    receipt.sequence = None;
    receipt.receipt_target = Some(MessageId::from_u128(5));
    receipt.payload.clear();
    assert_eq!(
        InnerEnvelopeV1::decode(&receipt.encode().unwrap()).unwrap(),
        receipt
    );
    for n in 0..4 {
        let mut i = receipt.clone();
        match n {
            0 => i.receipt_target = None,
            1 => i.receipt_target = Some(MessageId::from_u128(0)),
            2 => i.sequence = Some(SenderSequence::new(1)),
            3 => i.payload.push(0),
            _ => unreachable!(),
        };
        assert!(i.encode().is_err());
    }
}
