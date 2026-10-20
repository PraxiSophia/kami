use core::fmt;

/// Safe field classes, never wire values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Field {
    Version,
    Kind,
    MessageId,
    ConversationId,
    Sender,
    Recipient,
    Sequence,
    CreatedAt,
    Payload,
    Context,
    ReceiptTarget,
    MandatoryFeatures,
}

/// Diagnostics contain no payload, IDs, decoder source chains or untrusted values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolError {
    FrameTooLarge,
    MalformedWire,
    NonCanonicalWire,
    UnsupportedVersion,
    UnsupportedFeature,
    UnknownField,
    MissingField(Field),
    InvalidField(Field),
    InvalidKind,
    ContextMismatch(Field),
}
impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ProtocolError {}
