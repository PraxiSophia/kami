//! Protected one-device enrollment, explicit contact trust and Peer-ID binding.
//!
//! Native identity seeds cannot be read by downstream safe Rust callers:
//! ```compile_fail
//! use kami_identity::{NativeStore, SecretStore, KeySlot, KeyPurpose};
//! use kami_key_provider::ProfileKeyId;
//! let slot = KeySlot::new(ProfileKeyId::new("0123456789abcdef0123456789abcdef").unwrap(), KeyPurpose::Identity);
//! let _seed = NativeStore::new().read(&slot);
//! ```
//!
//! Run synchronous custody calls on a bounded blocking service worker, never
//! inside a SQL transaction or async networking task. Enrollment and trust
//! repository traits are trusted service capabilities, not IPC input APIs.
//! Detached invitations/proofs do not authenticate a live peer or establish human
//! trust. The sealed transport adapter constructs credentials/transports but owns
//! no sockets, tasks, E2EE sessions, storage worker or SQL implementation.
mod binding;
pub use binding::{verify_peer_binding, PeerBindingProof, MAX_BINDING_BYTES};
mod olm_binding;
pub use olm_binding::{
    verify_olm_binding, OlmBindingData, OlmBindingProof, OlmPrekeyKind, MAX_OLM_BINDING_BYTES,
    MAX_OLM_BINDING_VALIDITY_MS, MAX_OLM_PREKEY_REFERENCE_BYTES,
};
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
mod native_custody;
pub use native_custody::NativeCustody;
mod transport_credentials;
pub use transport_credentials::SealedTransportCredentials;
mod verified_enrollment;
pub use store::{KeyPurpose, KeySlot, SecretStore};
pub use verified_enrollment::{
    begin_fresh_native, complete_fresh_enrollment, validate_enrollment_snapshot,
    verify_enrollment_for_transport, FreshEnrollment, VerifiedEnrollment,
};
