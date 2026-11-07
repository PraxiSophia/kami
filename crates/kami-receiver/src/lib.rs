//! Runtime-neutral authenticated inbox coordination. No transport or scheduling.
mod context;
mod error;
mod receive;
pub use context::ReceiveContext;
pub use error::Error;
pub use kami_storage::{DurableDelivery, GapState, OrderedMessage};
pub use receive::{ReceiveOutcome, Receiver};
