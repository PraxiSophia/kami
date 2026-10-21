//! Protected one-device enrollment, explicit contact trust and Peer-ID binding.
//!
//! Run synchronous custody calls on a bounded blocking service worker, never
//! inside a SQL transaction or async networking task. Enrollment and trust
//! repository traits are trusted service capabilities, not IPC input APIs.
//! Detached invitations/proofs do not authenticate a live peer or establish human
//! trust. No E2EE, session, networking or SQL implementation lives in this crate.
mod binding;
pub use binding::{verify_peer_binding, PeerBindingProof, MAX_BINDING_BYTES};
mod descriptor;
pub use descriptor::{Invitation, MAX_INVITATION_BYTES};
mod trust;
pub use trust::{
    send_decision, ContactTrust, ContactTrustRepository, ContactTrustState, SendDecision,
    TrustTransition, VerificationEvidence,
};
mod error;
mod fingerprint;
pub use error::IdentityError;
pub use fingerprint::DeviceFingerprint;
mod custody;
mod enrollment;
mod native;
mod store;
pub use custody::Custody;
pub use enrollment::{EnrollmentJournal, EnrollmentRecord, EnrollmentState};
pub use native::NativeStore;
pub use store::{KeyPurpose, KeySlot, SecretStore};
