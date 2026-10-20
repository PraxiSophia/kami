use kami_identity::{KeyPurpose, KeySlot, NativeStore, SecretStore};
use kami_key_provider::{KeyError, ProfileKeyId};

#[test]
fn native_diagnostics_are_redacted() {
    assert_eq!(
        format!("{:?}", NativeStore::new()),
        "NativeStore([REDACTED])"
    );
}

#[test]
#[cfg(target_os = "linux")]
fn headless_linux_is_unavailable_not_absent() {
    const CHILD: &str = "KAMI_IDENTITY_HEADLESS_TEST_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let slot = KeySlot::new(
            ProfileKeyId::new("ffffffffffffffffffffffffffffffff").unwrap(),
            KeyPurpose::Database,
        );
        assert_eq!(
            NativeStore::new().read(&slot).err(),
            Some(KeyError::Unavailable)
        );
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "headless_linux_is_unavailable_not_absent",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            "unix:path=/nonexistent-kami-qualification-bus",
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

/// Read-only native probe; never creates or deletes credentials.
#[test]
#[ignore = "requires a native logged-in secure-store session; read-only probe"]
fn native_absent_slot_probe() {
    let slot = KeySlot::new(
        ProfileKeyId::new("fffffffffffffffffffffffffffffffe").unwrap(),
        KeyPurpose::Database,
    );
    assert_eq!(
        NativeStore::new().read(&slot).err(),
        Some(KeyError::Missing)
    );
}
