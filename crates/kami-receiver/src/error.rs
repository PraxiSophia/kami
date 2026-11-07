#[derive(Debug)]
pub enum Error {
    InvalidInput,
    Unauthorized,
    Conflict,
    SessionUnavailable,
    CryptoRejected,
    Storage(kami_storage::StorageError),
    Indeterminate(kami_storage::StorageError),
}
impl From<kami_storage::StorageError> for Error {
    fn from(e: kami_storage::StorageError) -> Self {
        match e {
            kami_storage::StorageError::Conflict => Self::Conflict,
            _ => Self::Storage(e),
        }
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "receiver: {self:?}")
    }
}
impl std::error::Error for Error {}
