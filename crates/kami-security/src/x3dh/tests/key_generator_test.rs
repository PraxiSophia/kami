use std::collections::HashSet;

use x25519_dalek::PublicKey;
use xeddsa::Verify;
use xeddsa::xed25519::PublicKey as XEdDSAPublicKey;

use super::key_generator::{
    KeyGenerator,
    KeyGeneratorError,
};


// TEST 1: Identity Key Generation
#[test]
fn test_generate_identity_key() {
    let identity = KeyGenerator::generate_identity_key();

    let public_key = identity.keypair.public_key_bytes();

    let expected_public_key = PublicKey::from(identity.keypair.private_key());

    assert_eq!(public_key, *expected_public_key.as_bytes());
}


// TEST 2: Signed Prekey Signature Verification
#[test]
fn test_signed_prekey_signature_verification() {
    let identity = KeyGenerator::generate_identity_key();

    let signed_prekey = KeyGenerator::generate_signed_prekey(&identity, 1);

    let identity_public_key = identity.keypair.public_key_bytes();

    let signed_prekey_public_key = signed_prekey.keypair.public_key_bytes();

    let verifying_key = XEdDSAPublicKey(identity_public_key);

    let result = verifying_key.verify(&signed_prekey_public_key, &signed_prekey.signature);

    assert!(result.is_ok(), "Signed Prekey signature verification failed");
}

// TEST 3: Tampered Signed Prekey Must Fail Verification
#[test]
fn test_tampered_signed_prekey_signature() {
    let identity = KeyGenerator::generate_identity_key();

    let signed_prekey = KeyGenerator::generate_signed_prekey(&identity, 1);

    let identity_public_key = identity.keypair.public_key_bytes();

    let mut tampered_public_key = signed_prekey.keypair.public_key_bytes();

    // Modify one byte of the signed message
    tampered_public_key[0] ^= 0x01;

    let verifying_key = XEdDSAPublicKey(identity_public_key);

    let result = verifying_key.verify(&tampered_public_key, &signed_prekey.signature);

    assert!(result.is_err(), "Tampered Signed Prekey was incorrectly accepted");
}


// TEST 4: One-Time Prekeys Generation
#[test]
fn test_generate_one_time_prekeys() {
    let start_id = 100;
    let count = 10;

    let prekeys = KeyGenerator::generate_one_time_prekeys(start_id, count)
        .expect("Failed to generate One-Time Prekeys");

    assert_eq!(prekeys.len(), count);

    let mut public_keys = HashSet::new();

    for (index, prekey) in prekeys.iter().enumerate() {
        assert_eq!(prekey.id, start_id + index as u32);

        let public_key = prekey.keypair.public_key_bytes();

        assert!(public_keys.insert(public_key), "Duplicate One-Time Prekey detected");
    }
}


// TEST 5: One-Time Prekey ID Overflow
#[test]
fn test_one_time_prekey_id_overflow() {
    let result =
        KeyGenerator::generate_one_time_prekeys(u32::MAX - 1, 3);

    assert!(matches!(result, Err(KeyGeneratorError::InvalidPrekeyRange)), "Expected InvalidPrekeyRange error");
}