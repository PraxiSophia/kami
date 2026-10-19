mod common;
use kami_protocol::{
    decode_frame, encode_frame, MessageId, MessageKind, SenderSequence, StructurallyValidFrame,
    MAX_ENCODED_FRAME_SIZE,
};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    fn raw_fuzz_input_never_panics_and_accepted_bytes_are_canonical(bytes in prop::collection::vec(any::<u8>(), 0..=65_537)) {
        if let Ok(frame) = decode_frame(&bytes) {
            prop_assert!(bytes.len() <= MAX_ENCODED_FRAME_SIZE);
            prop_assert_eq!(encode_frame(&frame).unwrap(), bytes);
        }
    }
    #[test]
    fn typed_roundtrip_is_deterministic(
        payload in prop::collection::vec(any::<u8>(), 1..=4096),
        context in prop::collection::vec(any::<u8>(), 16..=1024),
        sequence in 1u64..=u64::MAX,
        timestamp in 0u64..=253_402_300_799_999,
        receipt in any::<bool>(), id in any::<u128>(),
    ) {
        let mut input = common::input(&payload, &context);
        input.message_id = MessageId::from_u128(id);
        input.sequence = Some(SenderSequence::new(sequence));
        input.created_at_ms = timestamp;
        if receipt { input.kind = MessageKind::DeviceDelivered; input.sequence = None; input.receipt_target = Some(MessageId::from_u128(id)); }
        let frame = StructurallyValidFrame::new(input).unwrap();
        let bytes = encode_frame(&frame).unwrap();
        prop_assert_eq!(encode_frame(&frame).unwrap(), bytes.clone());
        prop_assert_eq!(decode_frame(&bytes).unwrap(), frame);
    }
    #[test]
    fn mutations_fail_or_have_exact_canonical_roundtrip(index in 0usize..160, value in any::<u8>()) {
        let mut bytes = common::golden_text();
        let i = index % bytes.len(); bytes[i] = value;
        if let Ok(frame) = decode_frame(&bytes) { prop_assert_eq!(encode_frame(&frame).unwrap(), bytes); }
    }
}
