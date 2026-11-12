use kami_e2ee::prekeys::{AccountLifecycle, MAX_INVENTORY};
#[test]
fn replenishment_preserves_exposed_selection_records() {
    let peer = libp2p_identity::Keypair::ed25519_from_bytes([1; 32])
        .unwrap()
        .public()
        .to_peer_id();
    let owner = AccountLifecycle::create([1; 32], peer, 1, 1_000, 30_000).unwrap();
    let next = owner.replenish(1, 1_001, 30_000).unwrap();
    for record in owner.inventory() {
        assert!(
            next.inventory().contains(record),
            "replenishment relabelled an outstanding selection"
        );
    }
}
#[test]
fn fallback_rotation_preserves_previous_until_expiry() {
    use kami_identity::OlmPrekeyKind;
    let peer = libp2p_identity::Keypair::ed25519_from_bytes([1; 32])
        .unwrap()
        .public()
        .to_peer_id();
    let owner = AccountLifecycle::create([1; 32], peer, 1, 1_000, 30_000).unwrap();
    let old = owner
        .inventory()
        .iter()
        .find(|p| p.prekey_kind == OlmPrekeyKind::Fallback)
        .unwrap()
        .clone();
    let rotated = owner.rotate_fallback(2_000, 30_000).unwrap();
    assert!(rotated.inventory().contains(&old));
    assert_eq!(
        rotated
            .inventory()
            .iter()
            .filter(|p| p.prekey_kind == OlmPrekeyKind::Fallback)
            .count(),
        2
    );
    assert!(rotated.rotate_fallback(30_999, 30_000).is_err());
    let next = rotated.rotate_fallback(31_000, 30_000).unwrap();
    assert!(!next.inventory().contains(&old));
    assert_eq!(next.one_time_count(), owner.one_time_count());
}
#[test]
fn generation_is_bounded_and_never_evicts_retained_keys() {
    let peer = libp2p_identity::Keypair::ed25519_from_bytes([1; 32])
        .unwrap()
        .public()
        .to_peer_id();
    let owner = AccountLifecycle::create([1; 32], peer, 1, 1_000, 30_000).unwrap();
    assert_eq!(owner.one_time_count(), 32);
    assert_eq!(owner.generation(), 1);
    assert!(owner.replenish(MAX_INVENTORY + 1, 1_001, 30_000).is_err());
    assert_eq!(owner.one_time_count(), 32);
    let next = owner.replenish(1, 1_001, 30_000).unwrap();
    assert_eq!(next.generation(), 2);
    assert_eq!(next.one_time_count(), 33);
    assert_eq!(owner.one_time_count(), 32);
}
