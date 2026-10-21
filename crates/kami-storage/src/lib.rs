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
pub use ordering::OrderedMessage;
mod worker;
pub use message::StoredMessage;

pub use error::StorageError;
pub use lease::ProfileLease;
pub use worker::{Config, Storage};
