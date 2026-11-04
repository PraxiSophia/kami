use ed25519_dalek::{Signer, SigningKey};
use kami_identity::{
    verify_olm_binding, ContactTrust, DeviceFingerprint, IdentityError, Invitation,
    TrustTransition, VerificationEvidence, MAX_OLM_BINDING_BYTES, MAX_OLM_BINDING_VALIDITY_MS,
    MAX_OLM_PREKEY_REFERENCE_BYTES,
};
use kami_identity::{
    Custody, EnrollmentRecord, KeyPurpose, KeySlot, OlmBindingData, OlmBindingProof, OlmPrekeyKind,
    SecretStore,
};
use kami_key_provider::{KeyError, ProfileKeyId};
use libp2p_identity::Keypair;
use zeroize::Zeroizing;

struct Store;
impl SecretStore for Store {
    fn read(&self, _: &KeySlot) -> Result<Zeroizing<Vec<u8>>, KeyError> {
        Ok(Zeroizing::new(vec![7; 32]))
    }
    fn create(&self, _: &KeySlot, _: &[u8]) -> Result<(), KeyError> {
        panic!("signing cannot provision")
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
fn data() -> OlmBindingData {
    OlmBindingData {
        publisher_peer: Keypair::ed25519_from_bytes([8; 32])
            .unwrap()
            .public()
            .to_peer_id(),
        olm_ed25519: SigningKey::from_bytes(&[9; 32]).verifying_key().to_bytes(),
        olm_curve25519: [10; 32],
        prekey_kind: OlmPrekeyKind::OneTime,
        prekey_reference: b"key-1".to_vec(),
        prekey_curve25519: [11; 32],
        binding_generation: 1,
        bundle_generation: 2,
        not_before_ms: 1_000,
        expires_at_ms: 2_000,
    }
}
fn peer_proof() -> kami_identity::PeerBindingProof {
    Custody::new(Store)
        .create_peer_binding(
            &record(),
            &Keypair::ed25519_from_bytes([8; 32]).unwrap().public(),
        )
        .unwrap()
}
#[test]
fn custody_signed_canonical_binding_roundtrips_and_verifies() {
    let data = data();
    let peer = peer_proof();
    let proof = Custody::new(Store)
        .create_olm_binding(&record(), &peer, data.clone())
        .unwrap();
    let parsed = OlmBindingProof::parse_bounded(&proof.to_bytes()).unwrap();
    assert_eq!(proof, parsed);
    assert_eq!(parsed.data(), &data);
    parsed
        .verify(*record().public_key(), &peer, &data, 1_500)
        .unwrap();
}

fn sign(data: OlmBindingData) -> OlmBindingProof {
    Custody::new(Store)
        .create_olm_binding(&record(), &peer_proof(), data)
        .unwrap()
}
fn verify(proof: &OlmBindingProof, expected: &OlmBindingData) -> Result<(), IdentityError> {
    proof.verify(*record().public_key(), &peer_proof(), expected, 1_500)
}

// Independent wire encoder: also permits invalid fields to reach the parser,
// rather than relying on the production constructor's validation.
fn wire(data: &OlmBindingData, device: [u8; 32]) -> Vec<u8> {
    let peer = data.publisher_peer.to_bytes();
    let mut bytes = b"kami-olm-binding-v1\0".to_vec();
    bytes.push(1);
    bytes.extend(device);
    bytes.extend((peer.len() as u16).to_be_bytes());
    bytes.extend(peer);
    bytes.extend(data.olm_ed25519);
    bytes.extend(data.olm_curve25519);
    bytes.push(data.prekey_kind as u8);
    bytes.extend((data.prekey_reference.len() as u16).to_be_bytes());
    bytes.extend(&data.prekey_reference);
    bytes.extend(data.prekey_curve25519);
    bytes.extend(data.binding_generation.to_be_bytes());
    bytes.extend(data.bundle_generation.to_be_bytes());
    bytes.extend(data.not_before_ms.to_be_bytes());
    bytes.extend(data.expires_at_ms.to_be_bytes());
    bytes.extend(SigningKey::from_bytes(&[7; 32]).sign(&bytes).to_bytes());
    bytes
}

#[test]
fn exact_transcript_matches_independent_encoder_and_public_verifier() {
    let proof = sign(data());
    let bytes = proof.to_bytes();
    assert_eq!(bytes, wire(&data(), *record().public_key()));
    let split = bytes.len() - 64;
    assert!(Keypair::ed25519_from_bytes([7; 32])
        .unwrap()
        .public()
        .verify(&bytes[..split], &bytes[split..]));
    assert_eq!(proof.device_key(), record().public_key());
    assert!(bytes.len() <= MAX_OLM_BINDING_BYTES);
}

#[test]
fn all_truncations_oversize_trailing_duplicate_unknown_fields_rejected() {
    let bytes = sign(data()).to_bytes();
    for length in 0..bytes.len() {
        assert!(
            OlmBindingProof::parse_bounded(&bytes[..length]).is_err(),
            "length {length}"
        );
    }
    assert_eq!(
        OlmBindingProof::parse_bounded(&vec![0; MAX_OLM_BINDING_BYTES + 1]),
        Err(IdentityError::Oversized)
    );
    // Fixed-order v1 has no extensibility area. Both before and after the
    // signature, repeated/unknown fields and padding must fail, not be ignored.
    for extra in [vec![0], vec![0x80, 0x01, 1], data().olm_ed25519.to_vec()] {
        for at in [bytes.len() - 64, bytes.len()] {
            let mut changed = bytes.clone();
            changed.splice(at..at, extra.clone());
            assert!(OlmBindingProof::parse_bounded(&changed).is_err());
        }
    }
}

#[test]
fn domain_version_signature_and_every_modified_byte_fail_closed() {
    let bytes = sign(data()).to_bytes();
    for index in 0..bytes.len() {
        let mut changed = bytes.clone();
        changed[index] ^= 1;
        if let Ok(proof) = OlmBindingProof::parse_bounded(&changed) {
            assert!(verify(&proof, &data()).is_err(), "byte {index}");
        }
    }
    let mut wrong_version = bytes.clone();
    wrong_version[b"kami-olm-binding-v1\0".len()] = 2;
    assert_eq!(
        OlmBindingProof::parse_bounded(&wrong_version),
        Err(IdentityError::UnsupportedVersion)
    );
    for domain in [
        b"kami-peer-binding-v1\0".as_slice(),
        b"kami-olm-binding-v2\0",
        b"kami-olm-binding-v1",
    ] {
        let mut wrong_transcript = domain.to_vec();
        wrong_transcript.extend(&bytes[b"kami-olm-binding-v1\0".len()..bytes.len() - 64]);
        let signature = SigningKey::from_bytes(&[7; 32])
            .sign(&wrong_transcript)
            .to_bytes();
        let mut changed = bytes.clone();
        let split = changed.len() - 64;
        changed[split..].copy_from_slice(&signature);
        assert_eq!(
            verify(&OlmBindingProof::parse_bounded(&changed).unwrap(), &data()),
            Err(IdentityError::ForgedBinding)
        );
    }
}

#[test]
fn pinned_device_and_peer_association_cannot_be_substituted() {
    let proof = sign(data());
    let other_device = SigningKey::from_bytes(&[12; 32]).verifying_key().to_bytes();
    assert_eq!(
        proof.verify(other_device, &peer_proof(), &data(), 1_500),
        Err(IdentityError::Changed)
    );
    let other_peer = Custody::new(Store)
        .create_peer_binding(
            &record(),
            &Keypair::ed25519_from_bytes([13; 32]).unwrap().public(),
        )
        .unwrap();
    assert_eq!(
        proof.verify(*record().public_key(), &other_peer, &data(), 1_500),
        Err(IdentityError::WrongPeer)
    );
    let mut wrong = data();
    wrong.publisher_peer = other_peer.peer_id();
    assert_eq!(
        Custody::new(Store).create_olm_binding(&record(), &peer_proof(), wrong.clone()),
        Err(IdentityError::WrongPeer)
    );
    let independently_signed =
        OlmBindingProof::parse_bounded(&wire(&wrong, *record().public_key())).unwrap();
    assert_eq!(
        verify(&independently_signed, &wrong),
        Err(IdentityError::WrongPeer)
    );
    // A forged existing-purpose proof is not accepted as an association.
    let mut bad_peer = peer_proof().to_bytes();
    *bad_peer.last_mut().unwrap() ^= 1;
    let bad_peer = kami_identity::PeerBindingProof::parse_bounded(&bad_peer).unwrap();
    assert_eq!(
        proof.verify(*record().public_key(), &bad_peer, &data(), 1_500),
        Err(IdentityError::ForgedBinding)
    );
    assert_eq!(
        Custody::new(Store).create_olm_binding(&record(), &bad_peer, data()),
        Err(IdentityError::ForgedBinding)
    );
}

#[test]
fn every_expected_account_prekey_generation_and_validity_field_is_enforced() {
    let proof = sign(data());
    let changes: Vec<OlmBindingData> = vec![
        OlmBindingData {
            olm_ed25519: SigningKey::from_bytes(&[14; 32]).verifying_key().to_bytes(),
            ..data()
        },
        OlmBindingData {
            olm_curve25519: [15; 32],
            ..data()
        },
        OlmBindingData {
            prekey_kind: OlmPrekeyKind::Fallback,
            ..data()
        },
        OlmBindingData {
            prekey_reference: b"other".to_vec(),
            ..data()
        },
        OlmBindingData {
            prekey_curve25519: [16; 32],
            ..data()
        },
        OlmBindingData {
            binding_generation: 2,
            ..data()
        },
        OlmBindingData {
            bundle_generation: 3,
            ..data()
        },
        OlmBindingData {
            not_before_ms: 999,
            ..data()
        },
        OlmBindingData {
            expires_at_ms: 2_001,
            ..data()
        },
    ];
    for expected in changes {
        assert_eq!(verify(&proof, &expected), Err(IdentityError::Conflict));
        // Even an authentic new publisher signature cannot override retained
        // selection/freshness policy for the old bundle (and vice versa).
        let replacement = sign(expected.clone());
        assert_eq!(verify(&replacement, &data()), Err(IdentityError::Conflict));
        verify(&replacement, &expected).unwrap();
    }
}

#[test]
fn fallback_and_one_time_are_distinct_signed_purposes() {
    let one_time = sign(data());
    let fallback_data = OlmBindingData {
        prekey_kind: OlmPrekeyKind::Fallback,
        ..data()
    };
    let fallback = sign(fallback_data.clone());
    assert_ne!(one_time.to_bytes(), fallback.to_bytes());
    verify(&fallback, &fallback_data).unwrap();
    assert_eq!(verify(&fallback, &data()), Err(IdentityError::Conflict));
    let mut swap = fallback.to_bytes();
    let signature = one_time.to_bytes();
    let split = swap.len() - 64;
    swap[split..].copy_from_slice(&signature[split..]);
    assert_eq!(
        verify(
            &OlmBindingProof::parse_bounded(&swap).unwrap(),
            &fallback_data
        ),
        Err(IdentityError::ForgedBinding)
    );
}

#[test]
fn generation_and_validity_bounds_apply_to_signing_and_untrusted_parser() {
    let mut invalid = Vec::new();
    for value in [0, i64::MAX as u64 + 1, u64::MAX] {
        invalid.push(OlmBindingData {
            binding_generation: value,
            ..data()
        });
        invalid.push(OlmBindingData {
            bundle_generation: value,
            ..data()
        });
    }
    invalid.extend([
        OlmBindingData {
            not_before_ms: 2_000,
            ..data()
        },
        OlmBindingData {
            not_before_ms: 2_001,
            ..data()
        },
        OlmBindingData {
            expires_at_ms: 1_000 + MAX_OLM_BINDING_VALIDITY_MS + 1,
            ..data()
        },
        OlmBindingData {
            expires_at_ms: u64::MAX,
            ..data()
        },
        OlmBindingData {
            not_before_ms: u64::MAX,
            ..data()
        },
    ]);
    for bad in invalid {
        assert_eq!(
            Custody::new(Store).create_olm_binding(&record(), &peer_proof(), bad.clone()),
            Err(IdentityError::Malformed)
        );
        assert_eq!(
            OlmBindingProof::parse_bounded(&wire(&bad, *record().public_key())),
            Err(IdentityError::Malformed)
        );
    }
    let limit = OlmBindingData {
        binding_generation: i64::MAX as u64,
        bundle_generation: i64::MAX as u64,
        expires_at_ms: 1_000 + MAX_OLM_BINDING_VALIDITY_MS,
        ..data()
    };
    verify(&sign(limit.clone()), &limit).unwrap();
    let maximum_time = OlmBindingData {
        not_before_ms: i64::MAX as u64 - 1,
        expires_at_ms: i64::MAX as u64,
        ..data()
    };
    sign(maximum_time.clone())
        .verify(
            *record().public_key(),
            &peer_proof(),
            &maximum_time,
            maximum_time.not_before_ms,
        )
        .unwrap();
    let proof = sign(data());
    for now in [0, 999, 2_000, u64::MAX] {
        assert_eq!(
            proof.verify(*record().public_key(), &peer_proof(), &data(), now),
            Err(IdentityError::Conflict)
        );
    }
    for now in [1_000, 1_999] {
        proof
            .verify(*record().public_key(), &peer_proof(), &data(), now)
            .unwrap();
    }
}

#[test]
fn prekey_reference_and_length_prefix_bounds_are_exact() {
    for length in [1, MAX_OLM_PREKEY_REFERENCE_BYTES] {
        let valid = OlmBindingData {
            prekey_reference: vec![0xff; length],
            ..data()
        };
        let proof = sign(valid.clone());
        verify(
            &OlmBindingProof::parse_bounded(&proof.to_bytes()).unwrap(),
            &valid,
        )
        .unwrap();
    }
    for (length, error) in [
        (0, IdentityError::Malformed),
        (MAX_OLM_PREKEY_REFERENCE_BYTES + 1, IdentityError::Oversized),
    ] {
        let bad = OlmBindingData {
            prekey_reference: vec![1; length],
            ..data()
        };
        assert_eq!(
            Custody::new(Store).create_olm_binding(&record(), &peer_proof(), bad.clone()),
            Err(error)
        );
        assert_eq!(
            OlmBindingProof::parse_bounded(&wire(&bad, *record().public_key())),
            Err(error)
        );
    }
    let bytes = sign(data()).to_bytes();
    let peer_len_at = b"kami-olm-binding-v1\0".len() + 1 + 32;
    let ref_len_at = peer_len_at + 2 + data().publisher_peer.to_bytes().len() + 32 + 32 + 1;
    for offset in [peer_len_at, ref_len_at] {
        let mut bad = bytes.clone();
        bad[offset..offset + 2].copy_from_slice(&u16::MAX.to_be_bytes());
        assert_eq!(
            OlmBindingProof::parse_bounded(&bad),
            Err(IdentityError::Oversized)
        );
    }
    let mut bad = bytes.clone();
    bad[ref_len_at - 1] = 3;
    assert_eq!(
        OlmBindingProof::parse_bounded(&bad),
        Err(IdentityError::Malformed)
    );
    // Nonminimal varint encoding of the first PeerId multihash code.
    let peer_at = peer_len_at + 2;
    let mut noncanonical = bytes.clone();
    noncanonical[peer_at] |= 0x80;
    noncanonical.insert(peer_at + 1, 0);
    noncanonical[peer_len_at + 1] += 1;
    assert!(OlmBindingProof::parse_bounded(&noncanonical).is_err());
}

#[test]
fn weak_and_noncanonical_ed25519_keys_are_rejected() {
    let mut noncanonical = [0xff; 32];
    noncanonical[0] = 0xed;
    noncanonical[31] = 0x7f;
    for key in [[0; 32], noncanonical] {
        let bad = OlmBindingData {
            olm_ed25519: key,
            ..data()
        };
        assert_eq!(
            Custody::new(Store).create_olm_binding(&record(), &peer_proof(), bad.clone()),
            Err(IdentityError::InvalidKey)
        );
        assert_eq!(
            OlmBindingProof::parse_bounded(&wire(&bad, *record().public_key())),
            Err(IdentityError::InvalidKey)
        );
        assert_eq!(
            OlmBindingProof::parse_bounded(&wire(&data(), key)),
            Err(IdentityError::InvalidKey)
        );
    }
}

fn contact() -> ContactTrust {
    let invitation = Invitation::new(*record().public_key(), peer_proof()).unwrap();
    let pending = ContactTrust::pending(invitation);
    let evidence = VerificationEvidence::independent_comparison(
        &pending,
        DeviceFingerprint::from_public_key(*record().public_key()).unwrap(),
        true,
    )
    .unwrap();
    pending
        .transition(TrustTransition::Verify(evidence))
        .unwrap()
}
#[test]
fn live_verifier_preserves_existing_no_tofu_trust_and_peer_contracts() {
    let contact = contact();
    let proof = sign(data());
    let expected = data();
    let check = |c: Option<&ContactTrust>, peer: Option<&libp2p_identity::PeerId>| {
        verify_olm_binding(c, *record().public_key(), &proof, &expected, peer, 1_500)
    };
    let peer = data().publisher_peer;
    check(Some(&contact), Some(&peer)).unwrap();
    assert_eq!(check(None, Some(&peer)), Err(IdentityError::Unknown));
    assert_eq!(
        check(Some(&contact), None),
        Err(IdentityError::Unauthenticated)
    );
    let wrong_peer = Keypair::ed25519_from_bytes([19; 32])
        .unwrap()
        .public()
        .to_peer_id();
    assert_eq!(
        check(Some(&contact), Some(&wrong_peer)),
        Err(IdentityError::WrongPeer)
    );
    let pending = ContactTrust::pending(contact.candidate().clone());
    assert_eq!(
        check(Some(&pending), Some(&peer)),
        Err(IdentityError::VerificationRequired)
    );
    let blocked = contact.transition(TrustTransition::Block).unwrap();
    assert_eq!(
        check(Some(&blocked), Some(&peer)),
        Err(IdentityError::Blocked)
    );
    let changed = blocked.transition(TrustTransition::Unblock).unwrap();
    assert_eq!(
        check(Some(&changed), Some(&peer)),
        Err(IdentityError::Changed)
    );
    assert_eq!(
        verify_olm_binding(
            Some(&contact),
            SigningKey::from_bytes(&[20; 32]).verifying_key().to_bytes(),
            &proof,
            &expected,
            Some(&peer),
            1_500
        ),
        Err(IdentityError::Changed)
    );
    // Recipient-local trust generation can advance independently, without
    // changing or re-signing the publisher's bundle generation.
    let evidence = VerificationEvidence::independent_comparison(
        &changed,
        DeviceFingerprint::from_public_key(*record().public_key()).unwrap(),
        true,
    )
    .unwrap();
    let reverified = changed
        .transition(TrustTransition::Verify(evidence))
        .unwrap();
    assert_ne!(contact.generation(), reverified.generation());
    check(Some(&reverified), Some(&peer)).unwrap();
}

#[test]
fn missing_locked_corrupt_custody_cannot_sign_or_replace_keys() {
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
            Custody::new(Failed(error)).create_olm_binding(&record(), &peer_proof(), data()),
            Err(IdentityError::Custody(error))
        );
    }
    struct Wrong;
    impl SecretStore for Wrong {
        fn read(&self, _: &KeySlot) -> Result<Zeroizing<Vec<u8>>, KeyError> {
            Ok(Zeroizing::new(vec![21; 32]))
        }
        fn create(&self, _: &KeySlot, _: &[u8]) -> Result<(), KeyError> {
            panic!("no writes")
        }
    }
    assert_eq!(
        Custody::new(Wrong).create_olm_binding(&record(), &peer_proof(), data()),
        Err(IdentityError::Custody(KeyError::Corrupt))
    );
}

#[test]
fn public_proof_and_diagnostics_do_not_export_private_identity_material() {
    let proof = sign(data());
    let bytes = proof.to_bytes();
    assert!(!bytes.windows(32).any(|w| w == [7; 32]));
    let expanded = SigningKey::from_bytes(&[7; 32]).to_keypair_bytes();
    assert!(!bytes.windows(expanded.len()).any(|w| w == expanded));
    assert_eq!(format!("{proof:?}"), "OlmBindingProof([REDACTED])");
    assert_eq!(format!("{:?}", proof.data()), "OlmBindingData([REDACTED])");
    assert_eq!(format!("{:?}", Custody::new(Store)), "Custody([REDACTED])");
    assert_eq!(
        format!("{}", IdentityError::ForgedBinding),
        "identity: ForgedBinding"
    );
}
