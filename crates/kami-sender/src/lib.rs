//! Runtime-neutral sender coordination over the quarantined E2EE/SQLCipher seam.
//! No sockets, timers, live end-peer provenance, receipts or Delivered authority.
mod coordinator;
mod error;
pub mod retry;
pub use coordinator::{Coordinator, EnqueueOutcome};
pub use error::Error;
pub use kami_storage::{Preparation, PreparedSend, QueuedSend, SendLease, SendRequest, SendState};
