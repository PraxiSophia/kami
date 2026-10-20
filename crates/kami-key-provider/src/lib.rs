//! Blocking, load-only database key custody boundary.
//!
//! No OS, SQL, protocol or async runtime dependency. Production composition
//! injects an OS-backed implementation; the opt-in `test-provider` feature must
//! be restricted to test/dev-dependencies and never selected as a fallback.
mod error;
mod profile;
mod secret;
#[cfg(feature = "test-provider")]
pub mod test_provider;
pub use error::KeyError;
pub use profile::ProfileKeyId;
pub use secret::DatabaseKey;

/// Blocking load-only API. Run on a bounded initialization/storage worker,
/// never on a network executor or inside a database transaction.
pub trait KeyProvider: Send + Sync {
    fn load_database_key(&self, profile: &ProfileKeyId) -> Result<DatabaseKey, KeyError>;
}
