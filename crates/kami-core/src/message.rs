//! Immutable authenticated-message facts and deduplication classification.
//! Authentication and atomic sender + message-ID lookup belong to adapters.

use crate::ids::{ConversationId, DeviceId, MessageId, SenderSequence};

/// Opaque binding of immutable application content, supplied by the protocol
/// security layer. No hashing, encryption, or authenticity is provided here.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ContentBinding([u8; 32]);

impl ContentBinding {
    pub const fn from_bytes(value: [u8; 32]) -> Self {
        Self(value)
    }
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Path-independent immutable identity and content of a logical message.
/// Constructors preserve values; they do not generate collision-resistant IDs
/// or authenticate untrusted input. Never allocate new facts for a retry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MessageFacts {
    message_id: MessageId,
    conversation_id: ConversationId,
    sender: DeviceId,
    recipient: DeviceId,
    sequence: SenderSequence,
    content_binding: ContentBinding,
}

impl MessageFacts {
    pub const fn new(
        message_id: MessageId,
        conversation_id: ConversationId,
        sender: DeviceId,
        recipient: DeviceId,
        sequence: SenderSequence,
        content_binding: ContentBinding,
    ) -> Self {
        Self {
            message_id,
            conversation_id,
            sender,
            recipient,
            sequence,
            content_binding,
        }
    }

    pub const fn message_id(self) -> MessageId {
        self.message_id
    }
    pub const fn conversation_id(self) -> ConversationId {
        self.conversation_id
    }
    pub const fn sender(self) -> DeviceId {
        self.sender
    }
    pub const fn recipient(self) -> DeviceId {
        self.recipient
    }
    pub const fn sequence(self) -> SenderSequence {
        self.sequence
    }
    pub const fn content_binding(self) -> ContentBinding {
        self.content_binding
    }

    /// Compare authenticated facts against one retained deduplication record.
    /// The persistence layer must atomically look up/retain the sender + ID key
    /// before accepting a new message or emitting a durable-delivery receipt.
    pub fn classify(&self, incoming: &Self) -> Result<MessageMatch, MessageConflict> {
        if self.sender != incoming.sender || self.message_id != incoming.message_id {
            Ok(MessageMatch::Distinct)
        } else if self == incoming {
            Ok(MessageMatch::Duplicate)
        } else {
            Err(MessageConflict)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessageMatch {
    Distinct,
    Duplicate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MessageConflict;
