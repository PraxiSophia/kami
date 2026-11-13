//! SQLCipher-only, synchronous-command storage on one bounded worker thread.
mod contact_trust;
pub mod prekeys;
pub use contact_trust::{ContactTrustRow, TrustState, MAX_CONTACT_DESCRIPTOR_BYTES};
mod enrollment;
mod recovery;
pub use recovery::{RecoveryStatus, ReestablishmentPreconditions};
mod error;
mod inbox;
mod lease;
mod message;
mod migrations;
mod open;
pub use inbox::Acceptance;
mod ordering;
mod outbox;
mod transactions;
// Private until provider imports, runtime provenance and rollback policy qualify.
#[allow(dead_code)]
mod session_state;
pub use ordering::{GapState, OrderedMessage, MAX_GAP_AGE_MS};
mod sender_state;
pub use sender_state::backoff_ms;
pub use sender_state::SendState;
pub use sender_state::{
    Preparation, PreparedSend, QueuedSend, SendRequest, PREPARATION_LIFETIME_MS,
};
pub use sender_state::{SendLease, ATTEMPT_LEASE_MS, MAX_ATTEMPTS};
mod receiver_state;
pub use receiver_state::{
    DurableDelivery, MAX_CONTACT_MESSAGES, MAX_CONVERSATION_MESSAGES, MAX_GAP_DISTANCE,
    MAX_UNRESOLVED_GAPS,
};
mod worker;
pub use message::StoredMessage;

pub use error::StorageError;
pub use lease::ProfileLease;
pub use worker::{Config, Storage};
