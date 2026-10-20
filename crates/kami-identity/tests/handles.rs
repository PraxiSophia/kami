use kami_identity::{EnrollmentRecord, KeyPurpose, KeySlot};
use kami_key_provider::{KeyError, ProfileKeyId};
use std::error::Error;

#[test]
fn durable_handles_are_versioned_namespace_separated_and_strict() {
    let profile = ProfileKeyId::new("0123456789abcdef0123456789abcdef").unwrap();
    let identity = KeySlot::new(profile.clone(), KeyPurpose::Identity);
    let database = KeySlot::new(profile, KeyPurpose::Database);
    for handle in [&identity, &database] {
        assert_eq!(
            KeySlot::from_persistent_string(&handle.to_persistent_string()).unwrap(),
            *handle
        );
        assert_eq!(format!("{handle:?}"), "KeySlot([REDACTED])");
    }
    assert_ne!(
        identity.to_persistent_string(),
        database.to_persistent_string()
    );
    for value in [
        "",
        "kami:identity:v2:0123456789abcdef0123456789abcdef",
        "other:identity:v1:0123456789abcdef0123456789abcdef",
        "kami:transport:v1:0123456789abcdef0123456789abcdef",
        "kami:identity:v1:../profile",
        "kami:identity:v1:0123456789abcdef0123456789abcdef:extra",
    ] {
        assert_eq!(
            KeySlot::from_persistent_string(value),
            Err(KeyError::Corrupt)
        );
    }
    assert_eq!(
        EnrollmentRecord::from_parts(identity, database, [0; 32]),
        Err(KeyError::Corrupt)
    );
}

#[test]
fn errors_never_retain_backend_sources_or_sensitive_diagnostics() {
    for error in [
        KeyError::Locked,
        KeyError::Unavailable,
        KeyError::Missing,
        KeyError::Corrupt,
        KeyError::PermissionDenied,
        KeyError::Other,
    ] {
        assert!(error.source().is_none());
        let diagnostic = format!("{error:?}: {error}");
        assert!(!diagnostic.contains("0123456789abcdef"));
        assert!(!diagnostic.contains("org.kami"));
        assert!(!diagnostic.contains("/home/"));
    }
    assert!(KeyError::Missing
        .to_string()
        .contains("explicit recovery required"));
}
