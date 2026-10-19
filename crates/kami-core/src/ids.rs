//! Opaque identifiers and checked per-sender sequence values.

use core::fmt;

/// Stable identifier for one logical message.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MessageId(u128);

impl MessageId {
    /// Creates an identifier from an already validated value.
    pub const fn from_u128(value: u128) -> Self {
        Self(value)
    }

    /// Returns the underlying identifier value.
    pub const fn as_u128(self) -> u128 {
        self.0
    }
}

impl fmt::Display for MessageId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:032x}", self.0)
    }
}

/// Monotonic sender sequence within one conversation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SenderSequence(u64);

impl SenderSequence {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
    /// Allocates the next value without wrapping. The caller must commit it
    /// atomically with a new logical message, scoped by sender/conversation.
    pub const fn checked_next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

/// Opaque conversation scope; generation/validation belongs to the caller.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ConversationId(u128);

impl ConversationId {
    pub const fn from_u128(value: u128) -> Self {
        Self(value)
    }
    pub const fn as_u128(self) -> u128 {
        self.0
    }
}

/// Opaque reference to a device's verified identity, not a display name or
/// transport address. This type does not perform identity verification.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DeviceId([u8; 32]);

impl DeviceId {
    pub const fn from_bytes(value: [u8; 32]) -> Self {
        Self(value)
    }
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_preserve_values() {
        let message_id = MessageId::from_u128(7);
        let sequence = SenderSequence::new(3);

        assert_eq!(message_id.as_u128(), 7);
        assert_eq!(message_id.to_string(), "00000000000000000000000000000007");
        assert_eq!(sequence.value(), 3);
    }
}
