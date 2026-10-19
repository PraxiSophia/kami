#![forbid(unsafe_code)]

//! Pure message-domain facts and deterministic outbox transitions.
//!
//! Callers own collision-resistant ID generation, per-sender/conversation
//! sequence allocation, identity/receipt authentication, durable transactions,
//! scheduling, and resource accounting. No type here proves cryptographic
//! authenticity or performs persistence. Replay committed events to reconstruct
//! a projection; do not reset attempt counters on retry or service restart.

mod delivery;
mod ids;
mod message;
mod queue;

pub use delivery::{
    AttemptFailure, Delivery, DeliveryEvent, DeliveryPath, DeliveryState, ReceiptEvidence,
    TransitionError,
};
pub use ids::{ConversationId, DeviceId, MessageId, SenderSequence};
pub use message::{ContentBinding, MessageConflict, MessageFacts, MessageMatch};
pub use queue::{QueueLimits, QueuePressure, QueueUsage};
