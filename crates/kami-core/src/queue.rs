//! Pure retained-record/byte admission accounting, not network frame limits.
//! Storage adapters must apply admission atomically with durable enqueue.

/// Explicit queue budget in retained messages and stored bytes. These are not
/// wire-size limits; the protocol adapter separately enforces encoded bounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueueLimits {
    messages: u64,
    bytes: u64,
}

impl QueueLimits {
    pub const fn new(messages: u64, bytes: u64) -> Self {
        Self { messages, bytes }
    }
}

/// Snapshot of durable queue usage, including held and retryable messages.
/// The storage adapter must apply admission atomically with enqueue. Retries
/// and duplicates do not reserve another slot. No accepted item is evicted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueueUsage {
    messages: u64,
    bytes: u64,
}

impl QueueUsage {
    pub const fn new(messages: u64, bytes: u64) -> Self {
        Self { messages, bytes }
    }
    pub const fn messages(self) -> u64 {
        self.messages
    }
    pub const fn bytes(self) -> u64 {
        self.bytes
    }

    pub fn admit(self, limits: QueueLimits, stored_bytes: u64) -> Result<Self, QueuePressure> {
        let messages = self
            .messages
            .checked_add(1)
            .filter(|count| *count <= limits.messages)
            .ok_or(QueuePressure::Messages)?;
        let bytes = self
            .bytes
            .checked_add(stored_bytes)
            .filter(|count| *count <= limits.bytes)
            .ok_or(QueuePressure::Bytes)?;
        Ok(Self { messages, bytes })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueuePressure {
    Messages,
    Bytes,
}
