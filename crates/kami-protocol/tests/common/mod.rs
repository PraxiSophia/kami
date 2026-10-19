#![allow(dead_code)]
use kami_core::{ConversationId, DeviceId};
use kami_protocol::{FrameInput, MessageId, MessageKind, SenderSequence};

pub fn input<'a>(payload: &'a [u8], context: &'a [u8]) -> FrameInput<'a> {
    FrameInput {
        message_id: MessageId::from_u128(1),
        conversation_id: ConversationId::from_u128(2),
        sender: DeviceId::from_bytes([3; 32]),
        recipient: DeviceId::from_bytes([4; 32]),
        kind: MessageKind::Text,
        sequence: Some(SenderSequence::new(1)),
        created_at_ms: 0,
        protected_payload: payload,
        authentication_context: context,
        receipt_target: None,
    }
}
pub fn golden_text() -> Vec<u8> {
    hex("080110011a10010101010101010101010101010101012210020202020202020202020202020202022a20030303030303030303030303030303030303030303030303030303030303030332200404040404040404040404040404040404040404040404040404040404040404380140014a01ff521005050505050505050505050505050505")
}
pub fn golden_receipt() -> Vec<u8> {
    hex("080110021a10010101010101010101010101010101012210020202020202020202020202020202022a2003030303030303030303030303030303030303030303030303030303030303033220040404040404040404040404040404040404040404040404040404040404040440014a01ff5210050505050505050505050505050505055a1006060606060606060606060606060606")
}
fn hex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap())
        .collect()
}
