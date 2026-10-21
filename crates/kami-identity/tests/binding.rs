mod common;
#[path = "common/vectors.rs"]
mod vectors;

#[test]
fn replacement_descriptor_vector() {
    let descriptor = common::invitation(3, 4);
    assert_eq!(descriptor.to_bytes(), vectors::REPLACEMENT);
    assert_eq!(
        kami_identity::Invitation::parse_bounded(vectors::REPLACEMENT).unwrap(),
        descriptor
    );
}

#[test]
fn canonical_transcript_interoperates_and_cross_domain_replay_is_rejected() {
    let bytes = vectors::BINDING;
    let fields = &bytes[1..bytes.len() - 64];
    let mut transcript = b"kami-peer-binding-v1\0".to_vec();
    transcript.extend(fields);
    let device = Keypair::ed25519_from_bytes([7; 32]).unwrap();
    assert!(device
        .public()
        .verify(&transcript, &bytes[bytes.len() - 64..]));
    assert_eq!(
        device.sign(&transcript).unwrap(),
        &bytes[bytes.len() - 64..]
    );
    for domain in [
        b"kami-device-fingerprint-v1\0".as_slice(),
        b"kami-peer-binding-v2\0",
        b"kami-peer-binding-v1",
    ] {
        let mut wrong = domain.to_vec();
        wrong.extend(fields);
        let signature = device.sign(&wrong).unwrap();
        let mut forged = bytes.to_vec();
        let split = forged.len() - 64;
        forged[split..].copy_from_slice(&signature);
        let proof = PeerBindingProof::parse_bounded(&forged).unwrap();
        assert_eq!(
            proof.verify_signature(vectors::DEVICE),
            Err(IdentityError::ForgedBinding)
        );
    }
    // Correct signature cannot be replayed onto another canonical transport key/PeerId.
    let mut other = vectors::ROTATED.to_vec();
    let split = other.len() - 64;
    other[split..].copy_from_slice(&bytes[bytes.len() - 64..]);
    assert_eq!(
        PeerBindingProof::parse_bounded(&other)
            .unwrap()
            .verify_signature(vectors::DEVICE),
        Err(IdentityError::ForgedBinding)
    );
}

#[test]
fn malformed_noncanonical_version_and_peer_derivation_fail_safely() {
    for length in 0..vectors::BINDING.len() {
        assert!(PeerBindingProof::parse_bounded(&vectors::BINDING[..length]).is_err());
    }
    let mut bytes = vectors::BINDING.to_vec();
    bytes[0] = 2;
    assert_eq!(
        PeerBindingProof::parse_bounded(&bytes),
        Err(IdentityError::UnsupportedVersion)
    );
    let mut bytes = vectors::BINDING.to_vec();
    bytes[1..3].copy_from_slice(&u16::MAX.to_be_bytes());
    assert_eq!(
        PeerBindingProof::parse_bounded(&bytes),
        Err(IdentityError::Oversized)
    );
    let mut bytes = vectors::BINDING.to_vec();
    bytes[45] ^= 1; // Peer-ID bytes, not the key; parsing/derivation must reject.
    assert!(PeerBindingProof::parse_bounded(&bytes).is_err());
    let mut bytes = vectors::BINDING.to_vec();
    bytes.push(0);
    assert_eq!(
        PeerBindingProof::parse_bounded(&bytes),
        Err(IdentityError::Malformed)
    );
    // Append a protobuf unknown field to the public key, fixing its length.
    let mut bytes = vectors::BINDING.to_vec();
    bytes.splice(39..39, [0x18, 0x01]);
    bytes[2] = 38;
    assert_eq!(
        PeerBindingProof::parse_bounded(&bytes),
        Err(IdentityError::Malformed)
    );
    let proof = PeerBindingProof::parse_bounded(vectors::BINDING).unwrap();
    assert_eq!(format!("{proof:?}"), "PeerBindingProof([REDACTED])");
    assert_eq!(
        format!("{}", IdentityError::ForgedBinding),
        "identity: ForgedBinding"
    );
}

#[test]
fn missing_locked_or_changed_custody_cannot_sign_or_provision() {
    struct Failed(KeyError);
    impl SecretStore for Failed {
        fn read(&self, _: &KeySlot) -> Result<Zeroizing<Vec<u8>>, KeyError> {
            Err(self.0)
        }
        fn create(&self, _: &KeySlot, _: &[u8]) -> Result<(), KeyError> {
            panic!("no writes")
        }
    }
    for error in [
        KeyError::Missing,
        KeyError::Locked,
        KeyError::Unavailable,
        KeyError::PermissionDenied,
        KeyError::Corrupt,
    ] {
        assert_eq!(
            Custody::new(Failed(error)).create_peer_binding(&record(), &transport(8)),
            Err(IdentityError::Custody(error))
        );
    }
    let old = record();
    let wrong = EnrollmentRecord::from_parts(
        old.identity_handle().clone(),
        old.database_handle().clone(),
        SigningKey::from_bytes(&[9; 32]).verifying_key().to_bytes(),
    )
    .unwrap();
    assert_eq!(
        Custody::new(Store).create_peer_binding(&wrong, &transport(8)),
        Err(IdentityError::Custody(KeyError::Corrupt))
    );
}

use ed25519_dalek::SigningKey;
use kami_identity::{
    verify_peer_binding, Custody, EnrollmentRecord, IdentityError, KeyPurpose, KeySlot,
    PeerBindingProof, SecretStore,
};
use kami_key_provider::{KeyError, ProfileKeyId};
use libp2p_identity::{Keypair, PeerId};
use zeroize::Zeroizing;

struct Store;
impl SecretStore for Store {
    fn read(&self, _: &KeySlot) -> Result<Zeroizing<Vec<u8>>, KeyError> {
        Ok(Zeroizing::new(vec![7; 32]))
    }
    fn create(&self, _: &KeySlot, _: &[u8]) -> Result<(), KeyError> {
        panic!("signing must not provision")
    }
}
fn record() -> EnrollmentRecord {
    let profile = ProfileKeyId::new("0123456789abcdef0123456789abcdef").unwrap();
    EnrollmentRecord::from_parts(
        KeySlot::new(profile.clone(), KeyPurpose::Identity),
        KeySlot::new(profile, KeyPurpose::Database),
        SigningKey::from_bytes(&[7; 32]).verifying_key().to_bytes(),
    )
    .unwrap()
}
fn transport(seed: u8) -> libp2p_identity::PublicKey {
    Keypair::ed25519_from_bytes([seed; 32]).unwrap().public()
}
#[test]
fn protected_binding_roundtrip_requires_actual_authenticated_peer() {
    let record = record();
    let public = transport(8);
    let proof = Custody::new(Store)
        .create_peer_binding(&record, &public)
        .unwrap();
    assert_eq!(record.public_key(), &vectors::DEVICE);
    assert_eq!(proof.to_bytes(), vectors::BINDING);
    assert_eq!(
        Custody::new(Store)
            .create_peer_binding(&record, &transport(9))
            .unwrap()
            .to_bytes(),
        vectors::ROTATED
    );
    let parsed = PeerBindingProof::parse_bounded(&proof.to_bytes()).unwrap();
    verify_peer_binding(*record.public_key(), &parsed, Some(&public.to_peer_id())).unwrap();
    assert_eq!(
        verify_peer_binding(*record.public_key(), &parsed, None),
        Err(IdentityError::Unauthenticated)
    );
    assert_eq!(
        verify_peer_binding(
            *record.public_key(),
            &parsed,
            Some(&transport(9).to_peer_id())
        ),
        Err(IdentityError::WrongPeer)
    );
    assert!(verify_peer_binding(
        SigningKey::from_bytes(&[9; 32]).verifying_key().to_bytes(),
        &parsed,
        Some(&public.to_peer_id())
    )
    .is_err());
    let mut forged = proof.to_bytes();
    *forged.last_mut().unwrap() ^= 1;
    let forged = PeerBindingProof::parse_bounded(&forged).unwrap();
    assert_eq!(
        verify_peer_binding(*record.public_key(), &forged, Some(&public.to_peer_id())),
        Err(IdentityError::ForgedBinding)
    );
    let _: PeerId = parsed.peer_id();
}
