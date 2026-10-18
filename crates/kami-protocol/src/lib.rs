#![forbid(unsafe_code)]

pub use kami_core::{MessageId, SenderSequence};

/// M1 protocol version boundary.
pub const M1_PROTOCOL_VERSION: u16 = 1;

/// Protocol message kinds reserved by the M1 contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessageKind {
    Text,
    DeviceDelivered,
}

#[cfg(test)]
mod tests {
    use super::{MessageKind, M1_PROTOCOL_VERSION};

    #[test]
    fn exposes_only_the_initial_protocol_boundary() {
        assert_eq!(M1_PROTOCOL_VERSION, 1);
        assert_eq!(MessageKind::Text, MessageKind::Text);
        assert_ne!(MessageKind::Text, MessageKind::DeviceDelivered);
    }
}
