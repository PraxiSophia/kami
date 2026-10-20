use crate::{
    scan, wire::WireFrame, Field as F, MessageKind, ProtocolError as E, MAX_ENCODED_FRAME_SIZE,
};
use core::fmt;
use kami_core::{ConversationId, DeviceId, MessageId, SenderSequence};

/// Borrowed outbound structural input. Opaque bytes must already be protected by a security adapter.
/// No Debug implementation: input may contain sensitive application material.
pub struct FrameInput<'a> {
    pub message_id: MessageId,
    pub conversation_id: ConversationId,
    pub sender: DeviceId,
    pub recipient: DeviceId,
    pub kind: MessageKind,
    pub sequence: Option<SenderSequence>,
    pub created_at_ms: u64,
    pub protected_payload: &'a [u8],
    pub authentication_context: &'a [u8],
    pub receipt_target: Option<MessageId>,
}

/// Caller-supplied expectations, NOT proof of a verified identity/session.
pub struct ExpectedContext {
    pub message_id: MessageId,
    pub conversation_id: ConversationId,
    pub sender: DeviceId,
    pub recipient: DeviceId,
    pub receipt_target: Option<MessageId>,
}

/// Structural validity is NOT authentication or delivery evidence.
#[derive(Clone, PartialEq)]
pub struct StructurallyValidFrame(pub(crate) WireFrame);
impl fmt::Debug for StructurallyValidFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("StructurallyValidFrame([REDACTED])")
    }
}
fn varint_len(value: u64) -> usize {
    ((64 - value.leading_zeros()).max(1) as usize).div_ceil(7)
}
fn bytes_len(len: usize) -> usize {
    1 + varint_len(len as u64) + len
}

impl StructurallyValidFrame {
    /// Checks all lengths and complete encoded size BEFORE copying input bytes.
    pub fn new(input: FrameInput<'_>) -> Result<Self, E> {
        if !(1..=scan::MAX_PAYLOAD).contains(&input.protected_payload.len()) {
            return Err(E::InvalidField(F::Payload));
        }
        if !(16..=scan::MAX_CONTEXT).contains(&input.authentication_context.len()) {
            return Err(E::InvalidField(F::Context));
        }
        if input.created_at_ms > scan::MAX_TIMESTAMP {
            return Err(E::InvalidField(F::CreatedAt));
        }
        let kind_len = match input.kind {
            MessageKind::Text => {
                let seq = input.sequence.ok_or(E::MissingField(F::Sequence))?;
                if seq.value() == 0 {
                    return Err(E::InvalidField(F::Sequence));
                }
                if input.receipt_target.is_some() {
                    return Err(E::InvalidField(F::ReceiptTarget));
                }
                1 + varint_len(seq.value())
            }
            MessageKind::DeviceDelivered => {
                if input.sequence.is_some() {
                    return Err(E::InvalidField(F::Sequence));
                }
                if input.receipt_target.is_none() {
                    return Err(E::MissingField(F::ReceiptTarget));
                }
                bytes_len(16)
            }
        };
        // All operands are capped above or fixed, so this sum cannot overflow usize.
        let size = 4
            + 2 * bytes_len(16)
            + 2 * bytes_len(32)
            + kind_len
            + 1
            + varint_len(input.created_at_ms)
            + bytes_len(input.protected_payload.len())
            + bytes_len(input.authentication_context.len());
        if size > MAX_ENCODED_FRAME_SIZE {
            return Err(E::FrameTooLarge);
        }
        Ok(Self(WireFrame {
            version: Some(1),
            kind: Some(match input.kind {
                MessageKind::Text => 1,
                MessageKind::DeviceDelivered => 2,
            }),
            message_id: Some(input.message_id.as_u128().to_be_bytes().to_vec()),
            conversation_id: Some(input.conversation_id.as_u128().to_be_bytes().to_vec()),
            sender: Some(input.sender.as_bytes().to_vec()),
            recipient: Some(input.recipient.as_bytes().to_vec()),
            sequence: input.sequence.map(SenderSequence::value),
            created_at: Some(input.created_at_ms),
            payload: Some(input.protected_payload.to_vec()),
            context: Some(input.authentication_context.to_vec()),
            receipt_target: input
                .receipt_target
                .map(|id| id.as_u128().to_be_bytes().to_vec()),
            mandatory_features: None,
        }))
    }
    pub fn message_id(&self) -> MessageId {
        MessageId::from_u128(id128(&self.0.message_id))
    }
    pub fn conversation_id(&self) -> ConversationId {
        ConversationId::from_u128(id128(&self.0.conversation_id))
    }
    pub fn sender(&self) -> DeviceId {
        DeviceId::from_bytes(fixed(&self.0.sender))
    }
    pub fn recipient(&self) -> DeviceId {
        DeviceId::from_bytes(fixed(&self.0.recipient))
    }
    pub fn kind(&self) -> MessageKind {
        if self.0.kind == Some(1) {
            MessageKind::Text
        } else {
            MessageKind::DeviceDelivered
        }
    }
    pub fn sequence(&self) -> Option<SenderSequence> {
        self.0.sequence.map(SenderSequence::new)
    }
    pub fn created_at_ms(&self) -> u64 {
        self.0.created_at.expect("validated timestamp")
    }
    pub fn protected_payload(&self) -> &[u8] {
        self.0.payload.as_deref().expect("validated payload")
    }
    pub fn authentication_context(&self) -> &[u8] {
        self.0.context.as_deref().expect("validated context")
    }
    pub fn receipt_target(&self) -> Option<MessageId> {
        self.0
            .receipt_target
            .as_ref()
            .map(|_| MessageId::from_u128(id128(&self.0.receipt_target)))
    }
}
fn fixed<const N: usize>(value: &Option<Vec<u8>>) -> [u8; N] {
    value
        .as_deref()
        .expect("validated field")
        .try_into()
        .expect("validated length")
}
fn id128(value: &Option<Vec<u8>>) -> u128 {
    u128::from_be_bytes(fixed(value))
}

/// Structural comparisons only. Does NOT verify ciphertext, context material, identity, or cryptography.
pub fn validate_for_context(
    frame: &StructurallyValidFrame,
    expected: &ExpectedContext,
) -> Result<(), E> {
    for (matches, field) in [
        (frame.message_id() == expected.message_id, F::MessageId),
        (
            frame.conversation_id() == expected.conversation_id,
            F::ConversationId,
        ),
        (frame.sender() == expected.sender, F::Sender),
        (frame.recipient() == expected.recipient, F::Recipient),
        (
            frame.receipt_target() == expected.receipt_target,
            F::ReceiptTarget,
        ),
    ] {
        if !matches {
            return Err(E::ContextMismatch(field));
        }
    }
    Ok(())
}
