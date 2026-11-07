#![doc = "Bounded internal E2EE provider/profile foundation."]

// Quarantined qualification boundary: deliberately not reachable from public
// provider operations until durable WP3 and runtime authentication are reviewed.
#[allow(dead_code)]
mod authenticated;
#[allow(dead_code)]
mod envelope;
#[allow(dead_code)]
mod persistence;

pub mod errors;
pub mod profile;
pub mod provider;
pub mod state;
pub mod types;
pub mod vodozemac;

pub use authenticated::{
    AuthenticatedCommit, AuthenticatedDelivery, AuthenticatedReceiptCommit, CommitMaterial,
    Endpoint, QuarantinedSession, ReplayPermit, SnapshotTransfer,
};
pub use errors::{Error, ProviderError};
pub use kami_core::{ConversationId, DeviceId, MessageId};
pub use profile::{Profile, ProfileVersion};
pub use provider::{DecryptResult, E2eeProvider, EncryptResult};
pub use state::{StateEnvelope, StateVersion};
pub use types::ApplicationContext;
pub use vodozemac::VodozemacProvider;

#[cfg(test)]
#[path = "../tests/qualification/envelope.rs"]
mod envelope_tests;

#[cfg(test)]
#[path = "../tests/qualification/authenticated.rs"]
mod authenticated_tests;
