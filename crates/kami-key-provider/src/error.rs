use std::fmt;

/// Safe diagnostic categories. Never retain a native error or its message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyError {
    Locked,
    Unavailable,
    Missing,
    Corrupt,
    PermissionDenied,
    Other,
}
impl fmt::Display for KeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Locked => "secure key store is locked",
            Self::Unavailable => "secure key store is unavailable",
            Self::Missing => "secure key is missing; explicit recovery required",
            Self::Corrupt => "secure key or reference is invalid",
            Self::PermissionDenied => "secure key access denied",
            Self::Other => "secure key operation failed",
        })
    }
}
impl std::error::Error for KeyError {}
