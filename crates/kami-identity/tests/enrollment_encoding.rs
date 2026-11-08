use kami_identity::validate_enrollment_snapshot;
use kami_key_provider::ProfileKeyId;

#[test]
fn enrollment_encoding_rejects_partial_noncanonical_foreign_and_invalid_keys() {
    let profile = ProfileKeyId::new("0123456789abcdef0123456789abcdef").unwrap();
    let public = ed25519_dalek::SigningKey::from_bytes(&[7; 32])
        .verifying_key()
        .to_bytes();
    let mut row = vec![1, 1];
    row.extend_from_slice(profile.as_str().as_bytes());
    row.extend_from_slice(&public);
    assert!(validate_enrollment_snapshot(&profile, &row).is_ok());
    for len in 0..row.len() {
        assert!(validate_enrollment_snapshot(&profile, &row[..len]).is_err());
    }
    for offset in [0, 1, 2] {
        let mut bad = row.clone();
        bad[offset] ^= 0x80;
        assert!(validate_enrollment_snapshot(&profile, &bad).is_err());
    }
    let mut bad = row.clone();
    bad.push(0);
    assert!(validate_enrollment_snapshot(&profile, &bad).is_err());
    let foreign = ProfileKeyId::new("1123456789abcdef0123456789abcdef").unwrap();
    assert!(validate_enrollment_snapshot(&foreign, &row).is_err());
    row[34..].fill(0);
    assert!(validate_enrollment_snapshot(&profile, &row).is_err());
}
