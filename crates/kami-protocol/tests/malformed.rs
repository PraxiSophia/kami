mod common;
use kami_protocol::{decode_frame, Field, ProtocolError as E};

// Split only the short fixed golden vector (all lengths/scalars are one byte).
fn rewrite(tag: u8, replacement: &[u8]) -> Vec<u8> {
    let bytes = common::golden_text();
    let mut result = Vec::new();
    let mut pos = 0;
    while pos < bytes.len() {
        let start = pos;
        let key = bytes[pos];
        pos += 1;
        let value = bytes[pos];
        pos += 1;
        if key & 7 == 2 {
            pos += usize::from(value);
        }
        if key >> 3 == tag {
            result.extend_from_slice(replacement);
        } else {
            result.extend_from_slice(&bytes[start..pos]);
        }
    }
    result
}

#[test]
fn complete_frame_field_policy() {
    for (tag, field) in [
        (1, Field::Version),
        (2, Field::Kind),
        (3, Field::MessageId),
        (4, Field::ConversationId),
        (5, Field::Sender),
        (6, Field::Recipient),
        (7, Field::Sequence),
        (8, Field::CreatedAt),
        (9, Field::Payload),
        (10, Field::Context),
    ] {
        assert_eq!(
            decode_frame(&rewrite(tag, &[])).unwrap_err(),
            E::MissingField(field)
        );
    }
    for (tag, field, len) in [
        (3, Field::MessageId, 15),
        (4, Field::ConversationId, 17),
        (5, Field::Sender, 31),
        (6, Field::Recipient, 33),
        (9, Field::Payload, 0),
        (10, Field::Context, 15),
    ] {
        let mut replacement = vec![(tag << 3) | 2, len];
        replacement.extend(vec![0; len.into()]);
        assert_eq!(
            decode_frame(&rewrite(tag, &replacement)).unwrap_err(),
            E::InvalidField(field)
        );
    }
    assert_eq!(
        decode_frame(&rewrite(7, &[56, 0])).unwrap_err(),
        E::InvalidField(Field::Sequence)
    );
    assert_eq!(
        decode_frame(&rewrite(2, &[16, 2])).unwrap_err(),
        E::InvalidField(Field::Sequence)
    );
    let mut bytes = common::golden_text();
    bytes.extend([96, 1]);
    assert_eq!(decode_frame(&bytes).unwrap_err(), E::UnsupportedFeature);
    *bytes.last_mut().unwrap() = 0;
    assert_eq!(decode_frame(&bytes).unwrap_err(), E::NonCanonicalWire);
    let mut bytes = common::golden_text();
    bytes.extend([90, 16]);
    bytes.extend([0; 16]);
    assert_eq!(
        decode_frame(&bytes).unwrap_err(),
        E::InvalidField(Field::ReceiptTarget)
    );
    let mut bytes = common::golden_receipt();
    bytes.truncate(bytes.len() - 18);
    assert_eq!(
        decode_frame(&bytes).unwrap_err(),
        E::MissingField(Field::ReceiptTarget)
    );
    let mut timestamp = vec![64];
    timestamp.extend([255; 9]);
    timestamp.push(1);
    assert_eq!(
        decode_frame(&rewrite(8, &timestamp)).unwrap_err(),
        E::InvalidField(Field::CreatedAt)
    );
    let mut context = vec![82, 129, 8];
    context.extend([0; 1025]);
    assert_eq!(
        decode_frame(&rewrite(10, &context)).unwrap_err(),
        E::InvalidField(Field::Context)
    );
}

#[test]
fn redaction_safe_diagnostics() {
    let sentinel = b"private-plaintext-key-credential";
    let frame =
        kami_protocol::StructurallyValidFrame::new(common::input(sentinel, sentinel)).unwrap();
    assert_eq!(format!("{frame:?}"), "StructurallyValidFrame([REDACTED])");
    let mut bytes = kami_protocol::encode_frame(&frame).unwrap();
    bytes.extend([104, 1]);
    let error = decode_frame(&bytes).unwrap_err();
    assert_eq!(format!("{error:?}"), "UnknownField");
    assert_eq!(format!("{error}"), "UnknownField");
    assert!(std::error::Error::source(&error).is_none());
}

#[test]
fn rejects_structural_violations() {
    for (bytes, expected) in [
        (vec![], E::MissingField(Field::Version)),
        (vec![8, 0], E::UnsupportedVersion),
        (vec![8, 2], E::UnsupportedVersion),
        (vec![8, 1, 16, 3], E::InvalidKind),
        (vec![8, 1, 16, 1], E::MissingField(Field::MessageId)),
        (
            vec![8, 1, 16, 1, 26, 1, 0],
            E::InvalidField(Field::MessageId),
        ),
    ] {
        assert_eq!(decode_frame(&bytes).unwrap_err(), expected);
    }
}

#[test]
fn rejects_ambiguous_or_malformed_protobuf() {
    for (bytes, expected) in [
        (vec![8, 1, 8, 1], E::NonCanonicalWire),
        (vec![8, 129, 0], E::NonCanonicalWire),
        (vec![16, 1, 8, 1], E::NonCanonicalWire),
        (vec![104, 1], E::UnknownField),
        (vec![10, 0], E::MalformedWire),
        (vec![8, 128], E::MalformedWire),
        (
            vec![26, 255, 255, 255, 255, 15],
            E::InvalidField(Field::MessageId),
        ),
        (vec![26, 16, 1], E::MalformedWire),
        (vec![0], E::MalformedWire),
        (
            vec![8, 255, 255, 255, 255, 255, 255, 255, 255, 255, 2],
            E::MalformedWire,
        ),
    ] {
        assert_eq!(decode_frame(&bytes).unwrap_err(), expected, "{bytes:?}");
    }
}
