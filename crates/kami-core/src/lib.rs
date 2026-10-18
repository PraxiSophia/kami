#![forbid(unsafe_code)]

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
}

#[cfg(test)]
mod tests {
    use super::{MessageId, SenderSequence};

    #[test]
    fn identifiers_preserve_values() {
        let message_id = MessageId::from_u128(7);
        let sequence = SenderSequence::new(3);

        assert_eq!(message_id.as_u128(), 7);
        assert_eq!(message_id.to_string(), "00000000000000000000000000000007");
        assert_eq!(sequence.value(), 3);
    }
}
