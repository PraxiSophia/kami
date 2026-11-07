//! Bounded Tokio orchestration over synchronous KAMI-24/25 and SQLCipher.
//! No transport, IPC, production evidence producer, or public authenticated ingress.
//! Events are bounded best-effort diagnostics; command replies and snapshots are
//! authoritative. A shutdown timeout does NOT stop blocking operations or release
//! profile ownership. Tokio runtime destruction may still wait for those operations.
mod engine;
mod ingress;
#[cfg(test)]
mod integration_tests;
pub mod relay;
#[cfg(test)]
mod tests;
mod worker;
pub use engine::Engine;
use kami_key_provider::{KeyProvider, ProfileKeyId};
use std::{path::PathBuf, sync::Arc, time::Duration};

pub struct Profile {
    pub directory: PathBuf,
    pub key_id: ProfileKeyId,
    pub key_provider: Arc<dyn KeyProvider>,
    /// Explicit enrollment only; never falls back from open to create.
    pub create: bool,
    pub storage: kami_storage::Config,
}
#[derive(Clone, Debug)]
pub struct Config {
    pub command_capacity: usize,
    pub event_capacity: usize,
    /// Includes the executing operation, not just queued operations.
    pub blocking_capacity: usize,
    pub shutdown_timeout: Duration,
    /// Poll durable eligibility (10 ms..=1 hour, default 1 second). Actual claims
    /// still need private live evidence and the existing atomic sender policy.
    pub retry_scan_interval: Duration,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            command_capacity: 32,
            event_capacity: 32,
            blocking_capacity: 8,
            shutdown_timeout: Duration::from_secs(10),
            retry_scan_interval: Duration::from_secs(1),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lifecycle {
    Starting,
    Reconciling,
    Running,
    Draining,
    Stopped,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShutdownOutcome {
    Clean,
    TimedOut,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidConfig,
    Busy,
    Closed,
    NotRunning,
    LiveTransportUnavailable,
    OperationRejected,
    /// Durable outcome unknown: retain stable IDs and reconcile; never re-encrypt.
    Indeterminate,
    /// Durable readback proved this attempt did not commit.
    RolledBack,
    Ownership,
    ProfileAlreadyOwned,
    Storage(kami_storage::StorageError),
    /// An admitted operation may have committed before failure; reconcile stable IDs.
    WorkerFailed,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "runtime: {self:?}")
    }
}
impl std::error::Error for Error {}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Lifecycle(Lifecycle),
    Reconciled(usize),
    RetryReady(usize),
    OperationCompleted,
    OperationFailed(Error),
    ShutdownTimedOut,
    Relay(relay::RelayEvent),
}
#[derive(Clone, Copy, Debug)]
pub struct Snapshot {
    pub lifecycle: Lifecycle,
    pub active_operations: usize,
    /// Engine-to-consumer diagnostic emissions dropped because the receiver is
    /// full/closed. Relay-internal telemetry is also best-effort, not authority.
    pub dropped_events: u64,
    pub recovered: usize,
    pub failure: Option<Error>,
    pub shutdown: Option<ShutdownOutcome>,
}
