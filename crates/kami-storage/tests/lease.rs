mod common;
use kami_storage::ProfileLease;
#[test]
fn exclusive_persistent_lease_releases_on_drop() {
    let dir = common::directory();
    let first = ProfileLease::acquire(dir.path()).unwrap();
    assert!(ProfileLease::acquire(dir.path()).is_err());
    drop(first);
    let second = ProfileLease::acquire(dir.path()).unwrap();
    assert!(dir.path().join("profile.lock").exists());
    drop(second);
}
