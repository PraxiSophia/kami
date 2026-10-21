use crate::{fingerprint::checked_key, IdentityError};
use ed25519_dalek::Signature;
use libp2p_identity::{PeerId, PublicKey};
use std::fmt;

const DOMAIN: &[u8] = b"kami-peer-binding-v1\0";
pub const MAX_BINDING_BYTES: usize = 325;
const MAX_KEY: usize = 128;
const MAX_PEER: usize = 128;

/// Detached signed association, NOT live authentication or human verification.
#[derive(Clone, Eq, PartialEq)]
pub struct PeerBindingProof {
    public: Vec<u8>,
    peer: PeerId,
    signature: [u8; 64],
}
impl PeerBindingProof {
    pub(crate) fn sign(
        public: &PublicKey,
        key: &ed25519_dalek::SigningKey,
    ) -> Result<Self, IdentityError> {
        use ed25519_dalek::Signer;
        let public_bytes = public.encode_protobuf();
        let mut proof = Self {
            public: public_bytes,
            peer: public.to_peer_id(),
            signature: [0; 64],
        };
        proof.validate_encoding()?;
        proof.signature = key.sign(&proof.transcript()).to_bytes();
        Ok(proof)
    }
    fn validate_encoding(&self) -> Result<(), IdentityError> {
        if self.public.len() > MAX_KEY || self.peer.to_bytes().len() > MAX_PEER {
            return Err(IdentityError::Oversized);
        }
        let public =
            PublicKey::try_decode_protobuf(&self.public).map_err(|_| IdentityError::Malformed)?;
        if public.encode_protobuf() != self.public {
            return Err(IdentityError::Malformed);
        }
        // v1 supports Ed25519 transport identities only; future algorithms need a reviewed profile.
        let ed = public
            .clone()
            .try_into_ed25519()
            .map_err(|_| IdentityError::InvalidKey)?;
        checked_key(&ed.to_bytes())?;
        if public.to_peer_id() != self.peer {
            return Err(IdentityError::WrongPeer);
        }
        Ok(())
    }
    fn fields(&self) -> Vec<u8> {
        let peer = self.peer.to_bytes();
        let mut out = Vec::with_capacity(4 + self.public.len() + peer.len());
        out.extend_from_slice(&(self.public.len() as u16).to_be_bytes());
        out.extend_from_slice(&self.public);
        out.extend_from_slice(&(peer.len() as u16).to_be_bytes());
        out.extend_from_slice(&peer);
        out
    }
    fn transcript(&self) -> Vec<u8> {
        let mut out = DOMAIN.to_vec();
        out.extend(self.fields());
        out
    }
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = vec![1];
        out.extend(self.fields());
        out.extend(self.signature);
        out
    }
    pub fn parse_bounded(bytes: &[u8]) -> Result<Self, IdentityError> {
        if bytes.len() > MAX_BINDING_BYTES {
            return Err(IdentityError::Oversized);
        }
        let (&version, mut remaining) = bytes.split_first().ok_or(IdentityError::Malformed)?;
        if version != 1 {
            return Err(IdentityError::UnsupportedVersion);
        }
        let public = take_field(&mut remaining, MAX_KEY)?.to_vec();
        let raw_peer = take_field(&mut remaining, MAX_PEER)?;
        let peer = PeerId::from_bytes(raw_peer).map_err(|_| IdentityError::Malformed)?;
        if peer.to_bytes() != raw_peer {
            return Err(IdentityError::Malformed);
        }
        let signature = remaining.try_into().map_err(|_| IdentityError::Malformed)?;
        let proof = Self {
            public,
            peer,
            signature,
        };
        proof.validate_encoding()?;
        Ok(proof)
    }
    pub fn peer_id(&self) -> PeerId {
        self.peer
    }
    /// Verify only the signed association; this never authenticates a connection.
    pub fn verify_signature(&self, device: [u8; 32]) -> Result<(), IdentityError> {
        self.validate_encoding()?;
        checked_key(&device)?
            .verify_strict(&self.transcript(), &Signature::from_bytes(&self.signature))
            .map_err(|_| IdentityError::ForgedBinding)
    }
}
pub(crate) fn take_field<'a>(
    remaining: &mut &'a [u8],
    max: usize,
) -> Result<&'a [u8], IdentityError> {
    let prefix = remaining.get(..2).ok_or(IdentityError::Malformed)?;
    let len = u16::from_be_bytes([prefix[0], prefix[1]]) as usize;
    if len > max {
        return Err(IdentityError::Oversized);
    }
    let value = remaining.get(2..2 + len).ok_or(IdentityError::Malformed)?;
    *remaining = &remaining[2 + len..];
    Ok(value)
}
/// Caller MUST supply the recipient PeerId from the authenticated secure connection,
/// including the end-to-end peer on relayed paths, never a descriptor/relay assertion.
pub fn verify_peer_binding(
    device: [u8; 32],
    proof: &PeerBindingProof,
    authenticated_peer: Option<&PeerId>,
) -> Result<(), IdentityError> {
    let authenticated = authenticated_peer.ok_or(IdentityError::Unauthenticated)?;
    if *authenticated != proof.peer {
        return Err(IdentityError::WrongPeer);
    }
    proof.verify_signature(device)
}
impl fmt::Debug for PeerBindingProof {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PeerBindingProof([REDACTED])")
    }
}
