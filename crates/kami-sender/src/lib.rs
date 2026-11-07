//! Runtime-neutral sender coordination over the quarantined E2EE/SQLCipher seam.
//! No sockets, timers, or live end-peer provenance. Receipts require E2EE authentication.
mod receipt;
pub use receipt::ReceiptOutcome;
mod coordinator;
mod error;
pub mod retry;
pub use coordinator::{Coordinator, EnqueueOutcome};
pub use error::Error;
pub use kami_storage::{Preparation, PreparedSend, QueuedSend, SendLease, SendRequest, SendState};
