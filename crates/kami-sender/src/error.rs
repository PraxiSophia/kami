use kami_storage::StorageError;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Storage(StorageError),
    CryptoRejected,
    RequestMismatch,
    IdentityHeld,
    /// Worker readback proves reservation is still uncommitted. Keep the same ID.
    RolledBack(StorageError),
    /// Do not encrypt or transmit speculatively; reopen/reconcile by stable ID.
    Indeterminate(StorageError),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "sender: {self:?}")
    }
}
impl std::error::Error for Error {}
impl From<StorageError> for Error {
    fn from(e: StorageError) -> Self {
        Self::Storage(e)
    }
}
