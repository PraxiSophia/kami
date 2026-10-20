use crate::StorageError;
use kami_core::{
    ContentBinding, ConversationId, DeviceId, MessageFacts, MessageId, SenderSequence,
};
use std::fmt;
use zeroize::Zeroizing;

/// Input must have already passed protocol, identity, authorization and E2EE validation.
/// This storage type does NOT establish authenticity. Retries reuse persisted bytes.
pub struct StoredMessage {
    facts: MessageFacts,
    payload: Zeroizing<Vec<u8>>,
}
impl StoredMessage {
    pub fn new(facts: MessageFacts, payload: Vec<u8>) -> Result<Self, StorageError> {
        let payload = Zeroizing::new(payload);
        if payload.is_empty()
            || payload.len() > 65_536
            || facts.sequence().value() == 0
            || facts.sequence().value() > i64::MAX as u64
        {
            return Err(StorageError::InvalidInput);
        }
        Ok(Self { facts, payload })
    }
    pub fn facts(&self) -> MessageFacts {
        self.facts
    }
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}
impl fmt::Debug for StoredMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("StoredMessage([REDACTED])")
    }
}
pub(crate) fn encode(f: MessageFacts) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(16 + 16 + 32 + 32 + 8 + 32);
    bytes.extend(f.message_id().as_u128().to_be_bytes());
    bytes.extend(f.conversation_id().as_u128().to_be_bytes());
    bytes.extend(f.sender().as_bytes());
    bytes.extend(f.recipient().as_bytes());
    bytes.extend(f.sequence().value().to_be_bytes());
    bytes.extend(f.content_binding().as_bytes());
    bytes
}
pub(crate) fn decode(mut bytes: &[u8]) -> Result<MessageFacts, StorageError> {
    fn take<const N: usize>(bytes: &mut &[u8]) -> Result<[u8; N], StorageError> {
        if bytes.len() < N {
            return Err(StorageError::WrongKeyOrCorrupt);
        }
        let (head, rest) = bytes.split_at(N);
        *bytes = rest;
        head.try_into().map_err(|_| StorageError::WrongKeyOrCorrupt)
    }
    let facts = MessageFacts::new(
        MessageId::from_u128(u128::from_be_bytes(take(&mut bytes)?)),
        ConversationId::from_u128(u128::from_be_bytes(take(&mut bytes)?)),
        DeviceId::from_bytes(take(&mut bytes)?),
        DeviceId::from_bytes(take(&mut bytes)?),
        SenderSequence::new(u64::from_be_bytes(take(&mut bytes)?)),
        ContentBinding::from_bytes(take(&mut bytes)?),
    );
    if !bytes.is_empty() {
        return Err(StorageError::WrongKeyOrCorrupt);
    }
    Ok(facts)
}
