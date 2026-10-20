mod common;
use common::*;
use kami_storage::{Config, ProfileLease, Storage};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[test]
fn crash_child() {
    let Some(path) = std::env::var_os("KAMI_TEST_PROFILE") else {
        return;
    };
    let path = std::path::PathBuf::from(path);
    let mode = std::env::var("KAMI_TEST_CRASH_MODE").unwrap();
    let s = Storage::create(
        ProfileLease::acquire(&path).unwrap(),
        profile(),
        Arc::new(Provider(Ok(42))),
        Config::default(),
    )
    .unwrap();
    if mode == "outbox" {
        s.enqueue(message(facts(1, 1))).unwrap();
    }
    if mode == "inbox" {
        s.accept(message(facts(1, 1)), 1).unwrap();
    }
    // A separate raw keyed connection is test-only, while the worker's lease stays held.
    let db = rusqlite::Connection::open(path.join("messages.db")).unwrap();
    db.execute_batch(&format!(
        "PRAGMA key=\"x'{}'\"; PRAGMA cipher_compatibility=4; PRAGMA temp_store=MEMORY;",
        "2a".repeat(32)
    ))
    .unwrap();
    if mode == "uncommitted" {
        db.execute_batch(
            "BEGIN IMMEDIATE; INSERT INTO sender_sequence VALUES(zeroblob(16),zeroblob(32),1);",
        )
        .unwrap();
    }
    std::fs::write(path.join("ready"), b"ready").unwrap();
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}
struct Child(std::process::Child);
impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[test]
fn kill_restart_recovers_committed_facts_and_discards_uncommitted_work() {
    for mode in ["outbox", "inbox", "uncommitted"] {
        let dir = directory();
        let mut child = Child(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "crash_child", "--nocapture"])
                .env("KAMI_TEST_PROFILE", dir.path())
                .env("KAMI_TEST_CRASH_MODE", mode)
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        while !dir.path().join("ready").exists() {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "child exited before ready"
            );
            assert!(Instant::now() < deadline, "child readiness timeout");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(ProfileLease::acquire(dir.path()).is_err());
        inspect(dir.path());
        child.0.kill().unwrap();
        child.0.wait().unwrap();
        let s = open(&dir);
        if mode == "outbox" {
            assert_eq!(s.pending(1).unwrap()[0].facts(), facts(1, 1));
        }
        if mode == "inbox" {
            assert_eq!(
                s.receipt(facts(1, 1).sender(), facts(1, 1).message_id())
                    .unwrap(),
                Some(facts(1, 1))
            );
            assert_eq!(
                s.accept(message(facts(1, 1)), 2).unwrap(),
                kami_storage::Acceptance::Duplicate
            );
        }
        if mode == "uncommitted" {
            assert!(s.pending(1).unwrap().is_empty());
            s.shutdown().unwrap();
            assert_eq!(
                raw(&dir)
                    .query_row("SELECT count(*) FROM sender_sequence", [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
    }
}
fn inspect(path: &std::path::Path) {
    let marker = b"SECRET-CONTENT-storage-regression-marker";
    let entries: Vec<_> = std::fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap())
        .collect();
    assert!(entries.iter().any(|e| e.file_name() == "messages.db-wal"));
    for entry in entries {
        let name = entry.file_name();
        let name = name.to_str().unwrap();
        assert!(
            [
                "profile.lock",
                "enrollment",
                "messages.db",
                "messages.db-wal",
                "messages.db-shm",
                "messages.db-journal",
                "ready"
            ]
            .contains(&name),
            "unexpected artifact {name}"
        );
        let bytes = std::fs::read(entry.path()).unwrap();
        assert!(
            !bytes.windows(marker.len()).any(|w| w == marker),
            "plaintext in {name}"
        );
        assert!(
            !bytes.windows(32).any(|w| w == [42; 32]),
            "fixture key in {name}"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if name != "ready" {
                assert_eq!(entry.metadata().unwrap().permissions().mode() & 0o077, 0);
            }
        }
    }
}
#[test]
fn rollback_journal_and_memory_temp_never_expose_payload_or_create_backups() {
    let dir = directory();
    let s = create(&dir);
    s.enqueue(message(facts(1, 1))).unwrap();
    s.shutdown().unwrap();
    let _lease = ProfileLease::acquire(dir.path()).unwrap();
    let db = raw(&dir);
    println!("SQLite runtime: {}", rusqlite::version());
    println!(
        "SQLCipher crypto provider: {}",
        db.pragma_query_value(None, "cipher_provider_version", |r| r.get::<_, String>(0))
            .unwrap()
    );
    db.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA temp_store=MEMORY; CREATE TEMP TABLE temporary(payload BLOB); INSERT INTO temporary VALUES('SECRET-CONTENT-storage-regression-marker'); BEGIN IMMEDIATE; UPDATE history SET payload=zeroblob(4096);").unwrap();
    assert_eq!(
        db.pragma_query_value(None, "temp_store", |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert!(db.query_row("SELECT sqlite_compileoption_used('TEMP_STORE=2') OR sqlite_compileoption_used('TEMP_STORE=3')",[],|r|r.get::<_,bool>(0)).unwrap());
    let bytes = std::fs::read(dir.path().join("messages.db-journal")).unwrap();
    assert!(!bytes
        .windows(b"SECRET-CONTENT-storage-regression-marker".len())
        .any(|w| w == b"SECRET-CONTENT-storage-regression-marker"));
    let names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert!(names.iter().all(|n| [
        "profile.lock",
        "enrollment",
        "messages.db",
        "messages.db-journal"
    ]
    .iter()
    .any(|expected| n == expected)));
    db.execute_batch("ROLLBACK").unwrap();
}
