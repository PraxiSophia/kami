//! Standalone safe allocation probe, run through resource_probe.py (not a Cargo integration test).
use allocation_counter::measure;
use kami_core::{ConversationId, DeviceId};
use kami_protocol::{
    decode_frame, encode_frame, FrameInput, MessageId, MessageKind, SenderSequence,
    StructurallyValidFrame, MAX_ENCODED_FRAME_SIZE as MAX,
};
use std::hint::black_box;

fn input(payload: &[u8]) -> FrameInput<'_> {
    FrameInput {
        message_id: MessageId::from_u128(1),
        conversation_id: ConversationId::from_u128(2),
        sender: DeviceId::from_bytes([3; 32]),
        recipient: DeviceId::from_bytes([4; 32]),
        kind: MessageKind::Text,
        sequence: Some(SenderSequence::new(1)),
        created_at_ms: 0,
        protected_payload: payload,
        authentication_context: &[5; 16],
        receipt_target: None,
    }
}
fn main() {
    let payload = vec![255; MAX];
    let (n, frame) = (MAX - 2048..MAX)
        .rev()
        .find_map(|n| {
            StructurallyValidFrame::new(input(&payload[..n]))
                .ok()
                .map(|f| (n, f))
        })
        .unwrap();
    let bytes = encode_frame(&frame).unwrap();
    assert_eq!(bytes.len(), MAX);
    let mut late_invalid = bytes.clone();
    // Last context length becomes too short: scanner must reject before any copies.
    late_invalid[MAX - 17] = 15;
    let over = vec![0; MAX + 1];
    let duplicate = vec![8; MAX];
    for (name, source) in [
        ("oversize", &over),
        ("late_invalid", &late_invalid),
        ("duplicate", &duplicate),
    ] {
        let info = measure(|| {
            assert!(black_box(decode_frame(black_box(source))).is_err());
        });
        assert_eq!(info.count_total, 0, "{name}");
        assert_eq!(info.bytes_max, 0, "{name}");
        println!("{name}: {info:?}");
    }
    let info = measure(|| {
        drop(black_box(decode_frame(black_box(&bytes)).unwrap()));
    });
    println!("max_valid_decode: {info:?}");
    // prost's optional Vec decoder copies via temporary Bytes: at most two
    // allocations per byte field (seven in receipts), each bounded by input.
    assert!(info.count_total <= 14);
    assert!(info.bytes_total <= (2 * MAX) as u64);
    assert!(info.bytes_max <= (2 * MAX) as u64);
    assert_eq!(info.bytes_current, 0);
    let info = measure(|| {
        drop(black_box(encode_frame(black_box(&frame)).unwrap()));
    });
    assert_eq!(info.count_total, 1);
    assert_eq!(info.bytes_max, MAX as u64);
    println!("max_valid_encode: {info:?}");
    let info = measure(|| {
        assert!(black_box(StructurallyValidFrame::new(input(&payload[..n + 1]))).is_err());
    });
    assert_eq!(info.count_total, 0);
    println!("oversize_outbound: {info:?}");
}
