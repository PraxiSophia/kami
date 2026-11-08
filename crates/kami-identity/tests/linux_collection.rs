#![cfg(target_os = "linux")]

use kami_identity::{KeyPurpose, KeySlot, NativeCustody};
use kami_key_provider::{KeyError, KeyProvider, ProfileKeyId};
use secret_service::{blocking::SecretService, EncryptionType};
use std::{
    collections::HashMap,
    io::Write,
    os::unix::fs::PermissionsExt,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Daemon(Child);
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Actual Secret Service collection-boundary regression. All credentials belong
/// to an isolated disposable keyring on a private bus, never the user's keyring.
/// Requires dbus-run-session and gnome-keyring-daemon; run explicitly on Linux.
#[test]
#[ignore = "requires dbus-run-session and gnome-keyring-daemon; isolated native fixture"]
fn linux_collection_boundary() {
    const CHILD: &str = "KAMI_PRIVATE_COLLECTION_TEST";
    if std::env::var_os(CHILD).is_none() {
        let root =
            std::env::temp_dir().join(format!("kami-private-keyring-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let output = Command::new("dbus-run-session")
            .arg("--")
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "linux_collection_boundary",
                "--ignored",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("HOME", &root)
            .env("XDG_DATA_HOME", root.join("data"))
            .env("XDG_CONFIG_HOME", root.join("config"))
            .env("XDG_RUNTIME_DIR", &root)
            .env_remove("GNOME_KEYRING_CONTROL")
            .output()
            .unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        assert!(
            output.status.success(),
            "private-bus fixture failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }

    let mut daemon = Daemon(
        Command::new("gnome-keyring-daemon")
            .args(["--foreground", "--unlock", "--components=secrets"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    // Only the disposable fixture's empty password; no real credentials used.
    daemon.0.stdin.take().unwrap().write_all(b"\n").unwrap();
    let bus = zbus::blocking::Connection::session().unwrap();
    let dbus = zbus::blocking::fdo::DBusProxy::new(&bus).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !dbus
        .name_has_owner("org.freedesktop.secrets".try_into().unwrap())
        .unwrap()
    {
        assert!(
            Instant::now() < deadline,
            "fixture did not own Secret Service"
        );
        assert!(daemon.0.try_wait().unwrap().is_none(), "fixture exited");
        std::thread::sleep(Duration::from_millis(20));
    }

    let service = SecretService::connect(EncryptionType::Dh).unwrap();
    // Fresh-only enrollment and native-anchor verification use the disposable store.
    let native = NativeCustody::open();
    let profile = ProfileKeyId::new("1123456789abcdef0123456789abcdef").unwrap();
    let fresh = kami_identity::begin_fresh_native(&native, &profile).unwrap();
    let row = fresh.canonical_row().to_vec();
    assert!(kami_identity::verify_enrollment_for_transport(&native, &profile, &row).is_err());
    assert!(kami_identity::begin_fresh_native(&native, &profile).is_err());
    kami_identity::complete_fresh_enrollment(&native, fresh, &row).unwrap();
    let proof = kami_identity::verify_enrollment_for_transport(&native, &profile, &row).unwrap();
    let credentials = proof.materialize(&native, &row).unwrap();
    let peer = credentials.local_peer_id();
    assert_eq!(credentials.invitation().binding().peer_id(), peer);
    let proof = kami_identity::verify_enrollment_for_transport(&native, &profile, &row).unwrap();
    assert_eq!(
        proof.materialize(&native, &row).unwrap().local_peer_id(),
        peer
    );
    let mut changed = row.clone();
    changed[34] ^= 1;
    assert!(kami_identity::verify_enrollment_for_transport(&native, &profile, &changed).is_err());
    assert!(kami_identity::verify_enrollment_for_transport(&native, &profile, &row).is_ok());
    let default = service.get_default_collection().unwrap();
    default.ensure_unlocked().unwrap();
    let session = service.get_collection_by_alias("session").unwrap();
    assert_ne!(default.collection_path, session.collection_path);
    let slot = KeySlot::new(
        ProfileKeyId::new("0123456789abcdef0123456789abcdef").unwrap(),
        KeyPurpose::Database,
    );
    let handle = slot.to_persistent_string();
    let attrs = || {
        HashMap::from([
            ("application", "org.kami.identity"),
            ("kami-handle", handle.as_str()),
        ])
    };
    session
        .create_item(
            "Disposable session key",
            attrs(),
            &[17; 32],
            false,
            "application/octet-stream",
        )
        .unwrap();
    // Prove the fixture reproduces the bug: global search finds the matching
    // session-only item even though the unlocked default collection is empty.
    assert!(default.search_items(attrs()).unwrap().is_empty());
    assert_eq!(service.search_items(attrs()).unwrap().unlocked.len(), 1);
    assert_eq!(
        NativeCustody::open()
            .load_database_key(slot.profile())
            .err(),
        Some(KeyError::Missing)
    );

    default
        .create_item(
            "Disposable durable key",
            attrs(),
            &[34; 32],
            false,
            "application/octet-stream",
        )
        .unwrap();
    // The session collision must neither shadow nor make the default ambiguous.
    assert_eq!(
        NativeCustody::open()
            .load_database_key(slot.profile())
            .unwrap()
            .with_bytes(|bytes| *bytes),
        [34; 32]
    );

    default
        .create_item(
            "Disposable duplicate",
            attrs(),
            &[51; 32],
            false,
            "application/octet-stream",
        )
        .unwrap();
    assert_eq!(
        NativeCustody::open()
            .load_database_key(slot.profile())
            .err(),
        Some(KeyError::Corrupt)
    );
    default.lock().unwrap();
    assert_eq!(
        NativeCustody::open()
            .load_database_key(slot.profile())
            .err(),
        Some(KeyError::Locked)
    );
}
