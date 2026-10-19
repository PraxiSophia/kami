mod common;
use kami_protocol::{
    decode_frame, encode_frame, ProtocolError as E, StructurallyValidFrame,
    MAX_ENCODED_FRAME_SIZE as MAX,
};

#[test]
fn exact_network_boundary_includes_envelope() {
    let payload = vec![255; MAX];
    let largest = (MAX - 2048..MAX)
        .rev()
        .find_map(|n| {
            StructurallyValidFrame::new(common::input(&payload[..n], &[5; 16]))
                .ok()
                .map(|f| (n, f))
        })
        .unwrap();
    let bytes = encode_frame(&largest.1).unwrap();
    assert_eq!(bytes.len(), MAX);
    assert_eq!(decode_frame(&bytes).unwrap(), largest.1);
    assert_eq!(
        StructurallyValidFrame::new(common::input(&payload[..largest.0 + 1], &[5; 16]))
            .unwrap_err(),
        E::FrameTooLarge
    );
    let mut over = bytes;
    over.push(0);
    assert_eq!(decode_frame(&over).unwrap_err(), E::FrameTooLarge);
}

#[test]
fn every_truncation_and_hostile_maximum_input_is_bounded() {
    let bytes = common::golden_text();
    for n in 0..bytes.len() {
        assert!(decode_frame(&bytes[..n]).is_err(), "prefix {n}");
    }
    for byte in [0, 8, 128, 255] {
        assert!(decode_frame(&vec![byte; MAX]).is_err());
        assert_eq!(
            decode_frame(&vec![byte; MAX + 1]).unwrap_err(),
            E::FrameTooLarge
        );
    }
}
