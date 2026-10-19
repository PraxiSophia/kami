use kami_core::*;

fn message() -> MessageFacts {
    MessageFacts::new(
        MessageId::from_u128(7),
        ConversationId::from_u128(8),
        DeviceId::from_bytes([1; 32]),
        DeviceId::from_bytes([2; 32]),
        SenderSequence::new(3),
        ContentBinding::from_bytes([4; 32]),
    )
}

#[test]
fn immutable_facts_preserve_domain_values() {
    let facts = message();
    assert_eq!(facts.message_id(), MessageId::from_u128(7));
    assert_eq!(facts.conversation_id().as_u128(), 8);
    assert_eq!(facts.sender().as_bytes(), &[1; 32]);
    assert_eq!(facts.recipient().as_bytes(), &[2; 32]);
    assert_eq!(facts.sequence().value(), 3);
    assert_eq!(facts.content_binding().as_bytes(), &[4; 32]);
}

#[test]
fn sender_sequence_advances_without_wrapping() {
    assert_eq!(
        SenderSequence::new(3).checked_next(),
        Some(SenderSequence::new(4))
    );
    assert_eq!(SenderSequence::new(u64::MAX).checked_next(), None);
}

#[test]
fn duplicate_requires_the_same_immutable_facts() {
    let original = message();
    assert_eq!(original.classify(&original), Ok(MessageMatch::Duplicate));
    let mut fields = [original; 6];
    fields[0] = MessageFacts::new(
        MessageId::from_u128(99),
        original.conversation_id(),
        original.sender(),
        original.recipient(),
        original.sequence(),
        original.content_binding(),
    );
    fields[1] = MessageFacts::new(
        original.message_id(),
        ConversationId::from_u128(99),
        original.sender(),
        original.recipient(),
        original.sequence(),
        original.content_binding(),
    );
    fields[2] = MessageFacts::new(
        original.message_id(),
        original.conversation_id(),
        DeviceId::from_bytes([99; 32]),
        original.recipient(),
        original.sequence(),
        original.content_binding(),
    );
    fields[3] = MessageFacts::new(
        original.message_id(),
        original.conversation_id(),
        original.sender(),
        DeviceId::from_bytes([99; 32]),
        original.sequence(),
        original.content_binding(),
    );
    fields[4] = MessageFacts::new(
        original.message_id(),
        original.conversation_id(),
        original.sender(),
        original.recipient(),
        SenderSequence::new(99),
        original.content_binding(),
    );
    fields[5] = MessageFacts::new(
        original.message_id(),
        original.conversation_id(),
        original.sender(),
        original.recipient(),
        original.sequence(),
        ContentBinding::from_bytes([99; 32]),
    );
    // Deduplication scope is authenticated sender + logical ID.
    assert_eq!(original.classify(&fields[0]), Ok(MessageMatch::Distinct));
    assert_eq!(original.classify(&fields[2]), Ok(MessageMatch::Distinct));
    for index in [1, 3, 4, 5] {
        assert_eq!(original.classify(&fields[index]), Err(MessageConflict));
    }
}
