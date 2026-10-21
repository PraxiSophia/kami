#![allow(dead_code)]
use ed25519_dalek::{Signer, SigningKey};
use kami_identity::{Invitation, PeerBindingProof};
use libp2p_identity::Keypair;

/// Independent implementation of the documented transcript, not custody internals.
pub fn invitation(device: u8, transport: u8) -> Invitation {
    let key = SigningKey::from_bytes(&[device; 32]);
    let public = Keypair::ed25519_from_bytes([transport; 32])
        .unwrap()
        .public();
    let encoded = public.encode_protobuf();
    let peer = public.to_peer_id().to_bytes();
    let mut fields = Vec::new();
    fields.extend((encoded.len() as u16).to_be_bytes());
    fields.extend(encoded);
    fields.extend((peer.len() as u16).to_be_bytes());
    fields.extend(peer);
    let mut transcript = b"kami-peer-binding-v1\0".to_vec();
    transcript.extend(&fields);
    let mut bytes = vec![1];
    bytes.extend(fields);
    bytes.extend(key.sign(&transcript).to_bytes());
    Invitation::new(
        key.verifying_key().to_bytes(),
        PeerBindingProof::parse_bounded(&bytes).unwrap(),
    )
    .unwrap()
}
