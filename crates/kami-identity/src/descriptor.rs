use crate::{
    binding::take_field, DeviceFingerprint, IdentityError, PeerBindingProof, MAX_BINDING_BYTES,
};
use std::fmt;

pub const MAX_INVITATION_BYTES: usize = 512;
/// Checked public descriptor, still UNTRUSTED. A valid self-signature is not
/// independent human verification, nor authentication of a live connection.
/// v1 intentionally carries no display/address/relay hints or executable payload.
#[derive(Clone, Eq, PartialEq)]
pub struct Invitation {
    device: [u8; 32],
    proof: PeerBindingProof,
}
impl Invitation {
    pub fn new(device: [u8; 32], proof: PeerBindingProof) -> Result<Self, IdentityError> {
        proof.verify_signature(device)?;
        Ok(Self { device, proof })
    }
    pub fn parse_bounded(bytes: &[u8]) -> Result<Self, IdentityError> {
        if bytes.len() > MAX_INVITATION_BYTES {
            return Err(IdentityError::Oversized);
        }
        if bytes.get(..4) != Some(b"KINV") {
            return Err(IdentityError::Malformed);
        }
        match bytes.get(4) {
            Some(1) => {}
            Some(_) => return Err(IdentityError::UnsupportedVersion),
            None => return Err(IdentityError::Malformed),
        }
        let device = bytes
            .get(5..37)
            .ok_or(IdentityError::Malformed)?
            .try_into()
            .map_err(|_| IdentityError::Malformed)?;
        let mut rest = &bytes[37..];
        let proof = PeerBindingProof::parse_bounded(take_field(&mut rest, MAX_BINDING_BYTES)?)?;
        if !rest.is_empty() {
            return Err(IdentityError::Malformed);
        }
        Self::new(device, proof)
    }
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = b"KINV\x01".to_vec();
        bytes.extend(self.device);
        let proof = self.proof.to_bytes();
        bytes.extend((proof.len() as u16).to_be_bytes());
        bytes.extend(proof);
        bytes
    }
    pub fn device_key(&self) -> &[u8; 32] {
        &self.device
    }
    pub fn binding(&self) -> &PeerBindingProof {
        &self.proof
    }
    pub fn fingerprint(&self) -> DeviceFingerprint {
        DeviceFingerprint::from_public_key(self.device).expect("checked descriptor key")
    }
}
impl fmt::Debug for Invitation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Invitation([REDACTED])")
    }
}
