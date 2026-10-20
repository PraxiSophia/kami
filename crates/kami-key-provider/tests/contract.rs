use kami_key_provider::{DatabaseKey, KeyError, KeyProvider, ProfileKeyId};

#[test]
fn profile_names_are_explicit_validated_and_redacted() {
    let id = ProfileKeyId::new("0123456789abcdef0123456789abcdef").unwrap();
    assert_eq!(id.as_str(), "0123456789abcdef0123456789abcdef");
    assert_eq!(format!("{id:?}"), "ProfileKeyId([REDACTED])");
    for invalid in [
        "",
        "default",
        "../profile",
        "ABCDEF0123456789abcdef0123456789ab",
        "0123456789abcdef0123456789abcde/",
    ] {
        assert_eq!(ProfileKeyId::new(invalid), Err(KeyError::Corrupt));
    }
    fn accepts_object(_: &dyn KeyProvider) {}
    let _ = accepts_object;
}

use zeroize::Zeroizing;

#[test]
fn database_key_is_owned_and_redacted() {
    let key = DatabaseKey::from_bytes(Zeroizing::new([0x41; 32]));
    assert_eq!(key.with_bytes(|bytes| bytes.len()), 32);
    assert_eq!(format!("{key:?}"), "DatabaseKey([REDACTED])");
    assert_eq!(format!("{key}"), "[REDACTED]");
}
