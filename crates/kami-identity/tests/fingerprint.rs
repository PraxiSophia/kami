use kami_identity::DeviceFingerprint;

#[test]
fn full_canonical_fingerprint_display_parses_for_independent_comparison() {
    let canonical = "126dea50 6043ab05 799ddbc6 de4d4e74 3a798401 ad1cfe32 5b4a9106 e6d84b67";
    let fingerprint = DeviceFingerprint::parse_display(canonical).unwrap();
    assert_eq!(fingerprint.to_string(), canonical);
    for invalid in [
        canonical.to_uppercase(),
        canonical.replace(' ', ""),
        format!(" {canonical}"),
        format!("{canonical} "),
        canonical[..8].to_string(),
        canonical.replace('1', "z"),
    ] {
        assert!(DeviceFingerprint::parse_display(&invalid).is_err());
    }
}

fn sha2_digest_for_test(key: &[u8; 32]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut transcript = b"kami-device-fingerprint-v1\0".to_vec();
    assert_eq!(transcript.len(), 27);
    transcript.extend(key);
    Sha256::digest(&transcript).into()
}

#[test]
fn noncanonical_edwards_encodings_are_rejected_before_fingerprinting() {
    let mut tested = 0;
    for offset in 0..19u8 {
        let mut bytes = [0xff; 32];
        bytes[0] = 0xed + offset;
        bytes[31] = 0x7f;
        if let Ok(key) = ed25519_dalek::VerifyingKey::from_bytes(&bytes) {
            if !key.is_weak() && key.to_edwards().compress().to_bytes() != bytes {
                tested += 1;
                assert!(DeviceFingerprint::from_public_key(bytes).is_err());
            }
        }
    }
    assert!(tested > 0);
}

#[test]
fn rfc8032_fingerprint_vector() {
    let key = [
        0xd7, 0x5a, 0x98, 0x01, 0x82, 0xb1, 0x0a, 0xb7, 0xd5, 0x4b, 0xfe, 0xd3, 0xc9, 0x64, 0x07,
        0x3a, 0x0e, 0xe1, 0x72, 0xf3, 0xda, 0xa6, 0x23, 0x25, 0xaf, 0x02, 0x1a, 0x68, 0xf7, 0x07,
        0x51, 0x1a,
    ];
    let fingerprint = DeviceFingerprint::from_public_key(key).unwrap();
    assert_eq!(
        fingerprint.to_string(),
        "126dea50 6043ab05 799ddbc6 de4d4e74 3a798401 ad1cfe32 5b4a9106 e6d84b67"
    );
    assert!(DeviceFingerprint::from_public_key([0; 32]).is_err());
    let second = [
        0x3d, 0x40, 0x17, 0xc3, 0xe8, 0x43, 0x89, 0x5a, 0x92, 0xb7, 0x0a, 0xa7, 0x4d, 0x1b, 0x7e,
        0xbc, 0x9c, 0x98, 0x2c, 0xcf, 0x2e, 0xc4, 0x96, 0x8c, 0xc0, 0xcd, 0x55, 0xf1, 0x2a, 0xf4,
        0x66, 0x0c,
    ];
    assert_eq!(
        DeviceFingerprint::from_public_key(second)
            .unwrap()
            .to_string(),
        "d6797e16 9ecccba7 ba7d1d10 61a1274d 5e5cce2a e5e34b2c 2dc67c8f b70ac232"
    );
    assert_eq!(fingerprint.as_bytes(), &sha2_digest_for_test(&key));
}
