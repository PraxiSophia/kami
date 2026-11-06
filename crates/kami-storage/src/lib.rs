//! SQLCipher-only, synchronous-command storage on one bounded worker thread.
mod contact_trust;
pub use contact_trust::{ContactTrustRow, TrustState, MAX_CONTACT_DESCRIPTOR_BYTES};
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
pub use ordering::OrderedMessage;
mod sender_state;
pub use sender_state::backoff_ms;
pub use sender_state::SendState;
pub use sender_state::{
    Preparation, PreparedSend, QueuedSend, SendRequest, PREPARATION_LIFETIME_MS,
};
pub use sender_state::{SendLease, ATTEMPT_LEASE_MS, MAX_ATTEMPTS};
mod worker;
pub use message::StoredMessage;

pub use error::StorageError;
pub use lease::ProfileLease;
pub use worker::{Config, Storage};
