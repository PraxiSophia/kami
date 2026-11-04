use crate::errors::Error;
use kami_core::{ConversationId, DeviceId, MessageId};

pub const MAX_CONTEXT_BYTES: usize = 512;
const CONTEXT_FIELD_COUNT: usize = 4;

/// Metadata awaiting a separately reviewed authenticated binding to ciphertext.
///
/// Deterministic serialization alone does not authenticate this context.
/// The current vodozemac adapter rejects public encryption/decryption rather
/// than returning advisory-only context alongside accepted cryptographic output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApplicationContext {
    pub conversation: ConversationId,
    pub message: MessageId,
    pub sender: DeviceId,
    pub recipient: DeviceId,
}

impl ApplicationContext {
    pub fn validate(&self) -> Result<(), Error> {
        if self.conversation.as_u128() == 0
            || self.message.as_u128() == 0
            || self.sender.as_bytes().iter().all(|byte| *byte == 0)
            || self.recipient.as_bytes().iter().all(|byte| *byte == 0)
        {
            return Err(Error::InvalidContext);
        }
        Ok(())
    }

    /// Encode a bounded, canonical context after validation.
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        self.validate()?;
        let fields = [
            self.conversation
                .as_u128()
                .to_be_bytes()
                .as_slice()
                .to_owned(),
            self.message.as_u128().to_be_bytes().as_slice().to_owned(),
            self.sender.as_bytes().to_vec(),
            self.recipient.as_bytes().to_vec(),
        ];
        let size = fields.iter().try_fold(0usize, |size, field| {
            size.checked_add(4 + field.len())
                .ok_or(Error::InvalidContext)
        })?;
        if size > MAX_CONTEXT_BYTES || fields.len() != CONTEXT_FIELD_COUNT {
            return Err(Error::InvalidContext);
        }
        let mut out = Vec::with_capacity(size);
        for field in fields {
            let len = u32::try_from(field.len()).map_err(|_| Error::InvalidContext)?;
            out.extend_from_slice(&len.to_be_bytes());
            out.extend_from_slice(&field);
        }
        Ok(out)
    }
}
