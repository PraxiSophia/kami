mod common;

fn constructed(receipt: bool) -> kami_protocol::StructurallyValidFrame {
    let mut input = common::input(&[255], &[5; 16]);
    input.message_id = MessageId::from_u128(u128::from_be_bytes([1; 16]));
    input.conversation_id = kami_core::ConversationId::from_u128(u128::from_be_bytes([2; 16]));
    input.created_at_ms = 1;
    if receipt {
        input.kind = MessageKind::DeviceDelivered;
        input.sequence = None;
        input.receipt_target = Some(MessageId::from_u128(u128::from_be_bytes([6; 16])));
    }
    kami_protocol::StructurallyValidFrame::new(input).unwrap()
}

use kami_protocol::{decode_frame, encode_frame, MessageId, MessageKind, SenderSequence};

#[test]
fn text_golden_roundtrip() {
    let bytes = common::golden_text();
    let frame = decode_frame(&bytes).expect("valid text golden");
    assert_eq!(encode_frame(&frame).unwrap(), bytes);
    assert_eq!(frame.kind(), MessageKind::Text);
    assert_eq!(frame.sequence(), Some(SenderSequence::new(1)));
    assert_eq!(frame.created_at_ms(), 1);
    assert_eq!(frame.authentication_context(), &[5; 16]);
    assert_eq!(frame.receipt_target(), None);
    assert_eq!(frame, constructed(false));
}
#[test]
fn receipt_golden_roundtrip() {
    let bytes = common::golden_receipt();
    let frame = decode_frame(&bytes).expect("valid receipt golden");
    assert_eq!(encode_frame(&frame).unwrap(), bytes);
    assert_eq!(frame.kind(), MessageKind::DeviceDelivered);
    assert_eq!(frame, constructed(true));
    assert_eq!(frame.sequence(), None);
    assert_eq!(
        frame.receipt_target(),
        Some(MessageId::from_u128(u128::from_be_bytes([6; 16])))
    );
}
