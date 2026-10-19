use kami_core::{ConversationId, DeviceId};
use kami_protocol::{
    decode_frame, encode_frame, validate_for_context, ExpectedContext, Field, FrameInput,
    MessageId, MessageKind, ProtocolError as E, SenderSequence, StructurallyValidFrame,
};

fn input<'a>(payload: &'a [u8], context: &'a [u8]) -> FrameInput<'a> {
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
#[test]
fn constructs_and_checks_context_without_authenticating() {
    let frame = StructurallyValidFrame::new(input(&[255], &[5; 16])).unwrap();
    let mut expected = ExpectedContext {
        message_id: MessageId::from_u128(1),
        conversation_id: ConversationId::from_u128(2),
        sender: DeviceId::from_bytes([3; 32]),
        recipient: DeviceId::from_bytes([4; 32]),
        receipt_target: None,
    };
    assert_eq!(validate_for_context(&frame, &expected), Ok(()));
    expected.recipient = DeviceId::from_bytes([9; 32]);
    assert_eq!(
        validate_for_context(&frame, &expected),
        Err(E::ContextMismatch(Field::Recipient))
    );
    assert_eq!(decode_frame(&encode_frame(&frame).unwrap()).unwrap(), frame);
    assert_eq!(frame.message_id(), MessageId::from_u128(1));
    assert_eq!(frame.protected_payload(), &[255]); // Opaque bytes, NOT UTF-8 text.
}
#[test]
fn all_expected_bindings_are_checked() {
    let frame = StructurallyValidFrame::new(input(&[255], &[5; 16])).unwrap();
    let expected = || ExpectedContext {
        message_id: MessageId::from_u128(1),
        conversation_id: ConversationId::from_u128(2),
        sender: DeviceId::from_bytes([3; 32]),
        recipient: DeviceId::from_bytes([4; 32]),
        receipt_target: None,
    };
    let mut value = expected();
    value.message_id = MessageId::from_u128(9);
    assert_eq!(
        validate_for_context(&frame, &value),
        Err(E::ContextMismatch(Field::MessageId))
    );
    let mut value = expected();
    value.conversation_id = ConversationId::from_u128(9);
    assert_eq!(
        validate_for_context(&frame, &value),
        Err(E::ContextMismatch(Field::ConversationId))
    );
    let mut value = expected();
    value.sender = DeviceId::from_bytes([9; 32]);
    assert_eq!(
        validate_for_context(&frame, &value),
        Err(E::ContextMismatch(Field::Sender))
    );
    let mut value = expected();
    value.receipt_target = Some(MessageId::from_u128(9));
    assert_eq!(
        validate_for_context(&frame, &value),
        Err(E::ContextMismatch(Field::ReceiptTarget))
    );
    let mut value = input(&[1], &[5; 16]);
    value.kind = MessageKind::DeviceDelivered;
    value.sequence = None;
    value.receipt_target = Some(MessageId::from_u128(9));
    let frame = StructurallyValidFrame::new(value).unwrap();
    let mut matching = expected();
    matching.receipt_target = Some(MessageId::from_u128(9));
    assert_eq!(validate_for_context(&frame, &matching), Ok(()));
    matching.receipt_target = Some(MessageId::from_u128(10));
    assert_eq!(
        validate_for_context(&frame, &matching),
        Err(E::ContextMismatch(Field::ReceiptTarget))
    );
}

#[test]
fn outbound_kind_and_boundaries_are_checked() {
    assert_eq!(kami_protocol::supported_versions(), &[1]);
    let mut value = input(&[1], &[5; 16]);
    value.created_at_ms = 253_402_300_800_000;
    assert_eq!(
        StructurallyValidFrame::new(value).unwrap_err(),
        E::InvalidField(Field::CreatedAt)
    );
    let mut value = input(&[1], &[5; 16]);
    value.sequence = None;
    assert_eq!(
        StructurallyValidFrame::new(value).unwrap_err(),
        E::MissingField(Field::Sequence)
    );
    let mut value = input(&[1], &[5; 16]);
    value.receipt_target = Some(MessageId::from_u128(9));
    assert_eq!(
        StructurallyValidFrame::new(value).unwrap_err(),
        E::InvalidField(Field::ReceiptTarget)
    );
    let mut value = input(&[1], &[5; 16]);
    value.kind = MessageKind::DeviceDelivered;
    assert_eq!(
        StructurallyValidFrame::new(value).unwrap_err(),
        E::InvalidField(Field::Sequence)
    );
    let mut value = input(&[1], &[5; 16]);
    value.kind = MessageKind::DeviceDelivered;
    value.sequence = None;
    assert_eq!(
        StructurallyValidFrame::new(value).unwrap_err(),
        E::MissingField(Field::ReceiptTarget)
    );
    assert_eq!(
        StructurallyValidFrame::new(input(&[1], &[5; 1025])).unwrap_err(),
        E::InvalidField(Field::Context)
    );
    assert_eq!(
        StructurallyValidFrame::new(input(&[1; 65537], &[5; 16])).unwrap_err(),
        E::InvalidField(Field::Payload)
    );
}
#[test]
fn rejects_invalid_outbound_input() {
    assert_eq!(
        StructurallyValidFrame::new(input(&[], &[5; 16])).unwrap_err(),
        E::InvalidField(Field::Payload)
    );
    assert_eq!(
        StructurallyValidFrame::new(input(&[1], &[5; 15])).unwrap_err(),
        E::InvalidField(Field::Context)
    );
    let mut value = input(&[1], &[5; 16]);
    value.sequence = Some(SenderSequence::new(0));
    assert_eq!(
        StructurallyValidFrame::new(value).unwrap_err(),
        E::InvalidField(Field::Sequence)
    );
}
