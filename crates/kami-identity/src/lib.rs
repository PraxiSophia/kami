//! Protected one-device enrollment and native credential adapters.
//!
//! Run synchronous custody calls on a bounded blocking service worker, never
//! inside a SQL transaction or async networking task. `EnrollmentJournal` is a
//! trusted service-owned held-lease/durable-marker capability; it is not an IPC
//! API. The default build has no test provider or plaintext fallback.
//!
//! This crate implements key custody only: public identity descriptors do not
//! themselves establish contact trust, Peer-ID bindings, or E2EE sessions.
mod custody;
mod enrollment;
mod native;
mod store;
pub use custody::Custody;
pub use enrollment::{EnrollmentJournal, EnrollmentRecord, EnrollmentState};
pub use native::NativeStore;
pub use store::{KeyPurpose, KeySlot, SecretStore};
