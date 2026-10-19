use std::time::{Duration, SystemTime};

use super::key_manager::{
    KeyManager,
    KeyManagerError,
    SignedPrekeyRotationPolicy,
};

const DAY: u64 = 24 * 60 * 60;

fn create_manager(policy: SignedPrekeyRotationPolicy) -> KeyManager {
    KeyManager::generate_initial_bundle(policy, 100)
        .expect("Failed to initialize KeyManager")
}

fn spk_created_at(manager: &KeyManager) -> u64 {
    manager
        .find_signed_prekey(1)
        .expect("Initial SPK must exist")
        .created_at
}

fn at_timestamp(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
}

// TEST 1: Initialize UserBundle
#[test]
fn test_generate_initial_bundle() {
    let manager = create_manager(SignedPrekeyRotationPolicy::SevenDays);

    assert_eq!(manager.get_one_time_prekey_count(), 100);

    assert_eq!(
        manager.get_rotation_policy(),
        SignedPrekeyRotationPolicy::SevenDays
    );

    assert!(manager.find_signed_prekey(1).is_some());
    assert!(manager.find_signed_prekey(2).is_none());

    assert!(manager.find_one_time_prekey(1).is_some());
    assert!(manager.find_one_time_prekey(100).is_some());
    assert!(manager.find_one_time_prekey(101).is_none());
}

// TEST 2: Retrieve Identity Public Key
#[test]
fn test_initial_identity_public_key() {
    let manager = create_manager(SignedPrekeyRotationPolicy::SevenDays);

    let public_key = manager.get_identity_public_key();

    assert_eq!(public_key.len(), 32);
}

// TEST 3: Reject Invalid Initial OPK Counts
#[test]
fn test_invalid_initial_opk_count() {
    for count in [0, 99, 10_001] {
        let result = KeyManager::generate_initial_bundle(
            SignedPrekeyRotationPolicy::SevenDays,
            count,
        );

        assert!(matches!(
            result,
            Err(KeyManagerError::OneTimePrekeyLimitNotSufficient)
        ));
    }

    assert!(KeyManager::generate_initial_bundle(
        SignedPrekeyRotationPolicy::SevenDays,
        10_000,
    ).is_ok());
}

// TEST 4: Validate SPK Rotation Policy Durations
#[test]
fn test_rotation_policy_durations() {
    assert_eq!(
        SignedPrekeyRotationPolicy::SevenDays.duration(),
        Duration::from_secs(7 * DAY)
    );

    assert_eq!(
        SignedPrekeyRotationPolicy::FifteenDays.duration(),
        Duration::from_secs(15 * DAY)
    );

    assert_eq!(
        SignedPrekeyRotationPolicy::ThirtyDays.duration(),
        Duration::from_secs(30 * DAY)
    );
}

// TEST 5: Update SPK Rotation Policy
#[test]
fn test_set_rotation_policy() {
    let mut manager = create_manager(SignedPrekeyRotationPolicy::ThirtyDays);

    manager.set_rotation_policy(SignedPrekeyRotationPolicy::SevenDays);

    assert_eq!(
        manager.get_rotation_policy(),
        SignedPrekeyRotationPolicy::SevenDays
    );
}

// TEST 6: Do Not Rotate SPK Before Deadline
#[test]
fn test_spk_rotation_before_deadline() {
    let mut manager = create_manager(SignedPrekeyRotationPolicy::SevenDays);

    let created_at = spk_created_at(&manager);
    let now = at_timestamp(created_at + 6 * DAY);

    assert!(!manager.should_rotate_signed_prekey(now));

    let rotated = manager.rotate_signed_prekey(now).unwrap();

    assert!(!rotated);
    assert!(manager.find_signed_prekey(1).is_some());
    assert!(manager.find_signed_prekey(2).is_none());
}

// TEST 7: Rotate SPK at Seven-Day Deadline
#[test]
fn test_spk_rotation_at_seven_days() {
    let mut manager = create_manager(SignedPrekeyRotationPolicy::SevenDays);

    let created_at = spk_created_at(&manager);
    let now = at_timestamp(created_at + 7 * DAY);

    assert!(manager.should_rotate_signed_prekey(now));

    let rotated = manager.rotate_signed_prekey(now).unwrap();

    assert!(rotated);

    // New active SPK.
    assert!(manager.find_signed_prekey(2).is_some());

    // Previous SPK retained.
    assert!(manager.find_signed_prekey(1).is_some());

    // New SPK timestamp.
    assert_eq!(
        manager.find_signed_prekey(2).unwrap().created_at,
        created_at + 7 * DAY
    );
}

// TEST 8: Rotate SPK at Fifteen-Day Deadline
#[test]
fn test_spk_rotation_at_fifteen_days() {
    let mut manager = create_manager(SignedPrekeyRotationPolicy::FifteenDays);

    let created_at = spk_created_at(&manager);
    let before = at_timestamp(created_at + 14 * DAY);
    let deadline = at_timestamp(created_at + 15 * DAY);

    assert!(!manager.should_rotate_signed_prekey(before));
    assert!(manager.should_rotate_signed_prekey(deadline));

    assert!(manager.rotate_signed_prekey(deadline).unwrap());
    assert!(manager.find_signed_prekey(2).is_some());
}

// TEST 9: Rotate SPK at Thirty-Day Deadline
#[test]
fn test_spk_rotation_at_thirty_days() {
    let mut manager = create_manager(SignedPrekeyRotationPolicy::ThirtyDays);

    let created_at = spk_created_at(&manager);
    let before = at_timestamp(created_at + 29 * DAY);
    let deadline = at_timestamp(created_at + 30 * DAY);

    assert!(!manager.should_rotate_signed_prekey(before));
    assert!(manager.should_rotate_signed_prekey(deadline));

    assert!(manager.rotate_signed_prekey(deadline).unwrap());
    assert!(manager.find_signed_prekey(2).is_some());
}

// TEST 10: Retain at Most Two Previous SPKs
#[test]
fn test_spk_rotation_retains_max_two_previous() {
    let mut manager = create_manager(SignedPrekeyRotationPolicy::SevenDays);

    let created_at = spk_created_at(&manager);

    // Rotation 1: Active=2, Previous=[1]
    manager
        .rotate_signed_prekey(at_timestamp(created_at + 7 * DAY))
        .unwrap();

    // Rotation 2: Active=3, Previous=[2,1]
    manager
        .rotate_signed_prekey(at_timestamp(created_at + 14 * DAY))
        .unwrap();

    assert!(manager.find_signed_prekey(1).is_some());
    assert!(manager.find_signed_prekey(2).is_some());
    assert!(manager.find_signed_prekey(3).is_some());

    // Rotation 3: Active=4, Previous=[3,2]
    manager
        .rotate_signed_prekey(at_timestamp(created_at + 21 * DAY))
        .unwrap();

    assert!(manager.find_signed_prekey(1).is_none());
    assert!(manager.find_signed_prekey(2).is_some());
    assert!(manager.find_signed_prekey(3).is_some());
    assert!(manager.find_signed_prekey(4).is_some());
}

// TEST 11: Prevent Duplicate SPK Rotation at Same Timestamp
#[test]
fn test_spk_rotation_does_not_rotate_twice() {
    let mut manager = create_manager(SignedPrekeyRotationPolicy::SevenDays);

    let created_at = spk_created_at(&manager);
    let now = at_timestamp(created_at + 7 * DAY);

    assert!(manager.rotate_signed_prekey(now).unwrap());

    // Same timestamp: no second rotation.
    assert!(!manager.rotate_signed_prekey(now).unwrap());

    assert!(manager.find_signed_prekey(2).is_some());
    assert!(manager.find_signed_prekey(3).is_none());
}

// TEST 12: Reject SPK Rotation with Invalid System Time
#[test]
fn test_spk_rotation_invalid_system_time() {
    let mut manager = create_manager(SignedPrekeyRotationPolicy::SevenDays);

    let invalid_time = SystemTime::UNIX_EPOCH - Duration::from_secs(1);

    assert!(matches!(
        manager.rotate_signed_prekey(invalid_time),
        Err(KeyManagerError::InvalidSystemTime)
    ));

    assert!(manager.find_signed_prekey(1).is_some());
    assert!(manager.find_signed_prekey(2).is_none());
}

// TEST 13: Find OPK by ID
#[test]
fn test_find_one_time_prekey() {
    let manager = create_manager(SignedPrekeyRotationPolicy::SevenDays);

    assert!(manager.find_one_time_prekey(1).is_some());
    assert!(manager.find_one_time_prekey(50).is_some());
    assert!(manager.find_one_time_prekey(100).is_some());
    assert!(manager.find_one_time_prekey(101).is_none());
}

// TEST 14: Consume OPK and Remove It from Local Bundle
#[test]
fn test_consume_one_time_prekey() {
    let mut manager = create_manager(SignedPrekeyRotationPolicy::SevenDays);

    assert_eq!(manager.get_one_time_prekey_count(), 100);

    let consumed = manager.consume_one_time_prekey(50);

    assert!(consumed.is_some());
    assert_eq!(manager.get_one_time_prekey_count(), 99);
    assert!(manager.find_one_time_prekey(50).is_none());
}

// TEST 15: Prevent Double Consumption of the Same OPK
#[test]
fn test_one_time_prekey_cannot_be_consumed_twice() {
    let mut manager = create_manager(SignedPrekeyRotationPolicy::SevenDays);

    assert!(manager.consume_one_time_prekey(10).is_some());

    // Second consumption must fail.
    assert!(manager.consume_one_time_prekey(10).is_none());
    assert_eq!(manager.get_one_time_prekey_count(), 99);
}

// TEST 16: Replenish OPKs Without Reusing Consumed IDs
#[test]
fn test_replenish_one_time_prekeys() {
    let mut manager = create_manager(SignedPrekeyRotationPolicy::SevenDays);

    // Consume 40 OPKs.
    for id in 1..=40 {
        assert!(manager.consume_one_time_prekey(id).is_some());
    }

    assert_eq!(manager.get_one_time_prekey_count(), 60);

    // Replenish back to 100.
    let generated = manager.replenish_one_time_prekeys(100).unwrap();

    assert_eq!(generated, 40);
    assert_eq!(manager.get_one_time_prekey_count(), 100);

    // Newly generated IDs must not reuse old IDs.
    assert!(manager.find_one_time_prekey(1).is_none());
    assert!(manager.find_one_time_prekey(40).is_none());
    assert!(manager.find_one_time_prekey(101).is_some());
    assert!(manager.find_one_time_prekey(140).is_some());
}

// TEST 17: Skip OPK Replenishment When Target Is Already Met
#[test]
fn test_replenish_one_time_prekeys_at_target() {
    let mut manager = create_manager(SignedPrekeyRotationPolicy::SevenDays);

    let generated = manager.replenish_one_time_prekeys(100).unwrap();

    assert_eq!(generated, 0);
    assert_eq!(manager.get_one_time_prekey_count(), 100);
}
