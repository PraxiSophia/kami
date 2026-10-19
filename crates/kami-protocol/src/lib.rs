#![forbid(unsafe_code)]

mod codec;
mod error;
mod frame;
mod scan;
mod wire;

pub use codec::{decode_frame, encode_frame, MAX_ENCODED_FRAME_SIZE};
pub use error::{Field, ProtocolError};
pub use frame::{validate_for_context, ExpectedContext, FrameInput, StructurallyValidFrame};

/// Supported profile versions; no implicit downgrade or feature negotiation.
pub const fn supported_versions() -> &'static [u16] {
    &[M1_PROTOCOL_VERSION]
}
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
