//! Purpose-limited Kami-device signatures over public Olm account/prekey data.
//!
//! Detached association only: no DH possession, human verification, revocation,
//! prekey consumption, durable anti-rollback, or live authentication is implied.
use crate::{binding::take_field, fingerprint::checked_key, IdentityError, PeerBindingProof};
use ed25519_dalek::{Signature, Signer, SigningKey};
use libp2p_identity::PeerId;
use std::fmt;

const DOMAIN: &[u8] = b"kami-olm-binding-v1\0";
const VERSION: u8 = 1;
const MAX_PEER: usize = 128;
pub const MAX_OLM_PREKEY_REFERENCE_BYTES: usize = 64;
/// Maximum signed validity interval: seven days, in milliseconds.
pub const MAX_OLM_BINDING_VALIDITY_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
/// Exact upper bound for v1, including domain, version and signature.
pub const MAX_OLM_BINDING_BYTES: usize = DOMAIN.len()
    + 1
    + 32
    + 2
    + MAX_PEER
    + 32
    + 32
    + 1
    + 2
    + MAX_OLM_PREKEY_REFERENCE_BYTES
    + 32
    + 8 * 4
    + 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum OlmPrekeyKind {
    OneTime = 1,
    Fallback = 2,
}

/// Public publisher-owned data. Generations are positive, at most i64::MAX,
/// and are NOT recipient-local ContactTrust generations. A recipient must retain
/// an authenticated expected generation/key selection to reject older bundles.
/// References are opaque nonempty bytes, not normalized text/base64.
/// Curve25519 keys are exact 32-byte provider encodings, never normalized here;
/// the provider still owns DH validation and contributory/possession checks.
#[derive(Clone, Eq, PartialEq)]
pub struct OlmBindingData {
    pub publisher_peer: PeerId,
    pub olm_ed25519: [u8; 32],
    pub olm_curve25519: [u8; 32],
    pub prekey_kind: OlmPrekeyKind,
    pub prekey_reference: Vec<u8>,
    pub prekey_curve25519: [u8; 32],
    pub binding_generation: u64,
    pub bundle_generation: u64,
    pub not_before_ms: u64,
    pub expires_at_ms: u64,
}
impl OlmBindingData {
    /// Unpredictable live-exchange challenge from the existing OS entropy source.
    pub fn new_challenge() -> Result<[u8; 32], IdentityError> {
        let mut challenge = [0; 32];
        getrandom::fill(&mut challenge)
            .map_err(|_| IdentityError::Custody(kami_key_provider::KeyError::Unavailable))?;
        if challenge == [0; 32] {
            return Err(IdentityError::Malformed);
        }
        Ok(challenge)
    }
    fn validate(&self) -> Result<(), IdentityError> {
        if self.publisher_peer.to_bytes().len() > MAX_PEER
            || self.prekey_reference.len() > MAX_OLM_PREKEY_REFERENCE_BYTES
        {
            return Err(IdentityError::Oversized);
        }
        checked_key(&self.olm_ed25519)?;
        if self.prekey_reference.is_empty()
            || !valid_generation(self.binding_generation)
            || !valid_generation(self.bundle_generation)
            || self.not_before_ms > i64::MAX as u64
            || self.expires_at_ms > i64::MAX as u64
            || self.expires_at_ms <= self.not_before_ms
            || self.expires_at_ms - self.not_before_ms > MAX_OLM_BINDING_VALIDITY_MS
        {
            return Err(IdentityError::Malformed);
        }
        Ok(())
    }
}
fn valid_generation(value: u64) -> bool {
    value > 0 && value <= i64::MAX as u64
}

/// Untrusted until verified against an independently pinned device and expected
/// public data. Parsing alone performs structural checks, not authentication.
#[derive(Clone, Eq, PartialEq)]
pub struct OlmBindingProof {
    device: [u8; 32],
    data: OlmBindingData,
    signature: [u8; 64],
}
impl OlmBindingProof {
    pub(crate) fn sign(data: OlmBindingData, key: &SigningKey) -> Result<Self, IdentityError> {
        data.validate()?;
        let mut proof = Self {
            device: key.verifying_key().to_bytes(),
            data,
            signature: [0; 64],
        };
        checked_key(&proof.device)?;
        proof.signature = key.sign(&proof.transcript()).to_bytes();
        Ok(proof)
    }
    /// v1 is fixed-order, not extensible TLV/protobuf: domain || version ||
    /// device[32] || peer_len:u16be || canonical PeerId || olm_ed[32] ||
    /// olm_curve[32] || kind:u8 || ref_len:u16be || ref || prekey[32] ||
    /// binding_gen:u64be || bundle_gen:u64be || not_before_ms:u64be ||
    /// expires_at_ms:u64be. Every byte, including version/domain, is signed.
    /// The wire representation appends signature[64] to that transcript.
    /// There are no optional, unknown or repeated fields and no trailing data.
    fn transcript(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(MAX_OLM_BINDING_BYTES);
        out.extend(DOMAIN);
        out.push(VERSION);
        out.extend(self.device);
        let peer = self.data.publisher_peer.to_bytes();
        out.extend((peer.len() as u16).to_be_bytes());
        out.extend(peer);
        out.extend(self.data.olm_ed25519);
        out.extend(self.data.olm_curve25519);
        out.push(self.data.prekey_kind as u8);
        out.extend((self.data.prekey_reference.len() as u16).to_be_bytes());
        out.extend(&self.data.prekey_reference);
        out.extend(self.data.prekey_curve25519);
        for value in [
            self.data.binding_generation,
            self.data.bundle_generation,
            self.data.not_before_ms,
            self.data.expires_at_ms,
        ] {
            out.extend(value.to_be_bytes());
        }
        out
    }
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = self.transcript();
        out.extend(self.signature);
        out
    }
    pub fn data(&self) -> &OlmBindingData {
        &self.data
    }
    pub fn device_key(&self) -> &[u8; 32] {
        &self.device
    }
    pub fn parse_bounded(bytes: &[u8]) -> Result<Self, IdentityError> {
        if bytes.len() > MAX_OLM_BINDING_BYTES {
            return Err(IdentityError::Oversized);
        }
        let mut remaining = bytes;
        if take_fixed::<{ DOMAIN.len() }>(&mut remaining)? != DOMAIN {
            return Err(IdentityError::Malformed);
        }
        if take_fixed::<1>(&mut remaining)? != [VERSION] {
            return Err(IdentityError::UnsupportedVersion);
        }
        let device = take_fixed(&mut remaining)?;
        checked_key(&device)?;
        let peer_bytes = take_field(&mut remaining, MAX_PEER)?;
        let publisher_peer =
            PeerId::from_bytes(peer_bytes).map_err(|_| IdentityError::Malformed)?;
        if publisher_peer.to_bytes() != peer_bytes {
            return Err(IdentityError::Malformed);
        }
        let olm_ed25519 = take_fixed(&mut remaining)?;
        let olm_curve25519 = take_fixed(&mut remaining)?;
        let prekey_kind = match take_fixed::<1>(&mut remaining)?[0] {
            1 => OlmPrekeyKind::OneTime,
            2 => OlmPrekeyKind::Fallback,
            _ => return Err(IdentityError::Malformed),
        };
        let prekey_reference = take_field(&mut remaining, MAX_OLM_PREKEY_REFERENCE_BYTES)?.to_vec();
        let data = OlmBindingData {
            publisher_peer,
            olm_ed25519,
            olm_curve25519,
            prekey_kind,
            prekey_reference,
            prekey_curve25519: take_fixed(&mut remaining)?,
            binding_generation: u64::from_be_bytes(take_fixed(&mut remaining)?),
            bundle_generation: u64::from_be_bytes(take_fixed(&mut remaining)?),
            not_before_ms: u64::from_be_bytes(take_fixed(&mut remaining)?),
            expires_at_ms: u64::from_be_bytes(take_fixed(&mut remaining)?),
        };
        data.validate()?;
        let signature = take_fixed(&mut remaining)?;
        if !remaining.is_empty() {
            return Err(IdentityError::Malformed);
        }
        let proof = Self {
            device,
            data,
            signature,
        };
        if proof.to_bytes() != bytes {
            return Err(IdentityError::Malformed);
        }
        Ok(proof)
    }
    /// Verify a detached association against a trusted pin and selected public
    /// bundle. `expected` MUST come from trusted selection/retained freshness
    /// policy, not merely `self.data()`. Validity is [not_before, expires_at), no
    /// implicit clock skew. This does not consume a prekey or persist freshness.
    pub fn verify(
        &self,
        pinned_device: [u8; 32],
        peer_binding: &PeerBindingProof,
        expected: &OlmBindingData,
        now_ms: u64,
    ) -> Result<(), IdentityError> {
        self.data.validate()?;
        if self.device != pinned_device {
            return Err(IdentityError::Changed);
        }
        peer_binding.verify_signature(pinned_device)?;
        if peer_binding.peer_id() != self.data.publisher_peer {
            return Err(IdentityError::WrongPeer);
        }
        checked_key(&pinned_device)?
            .verify_strict(&self.transcript(), &Signature::from_bytes(&self.signature))
            .map_err(|_| IdentityError::ForgedBinding)?;
        if &self.data != expected {
            return Err(IdentityError::Conflict);
        }
        if now_ms < self.data.not_before_ms || now_ms >= self.data.expires_at_ms {
            return Err(IdentityError::Conflict);
        }
        Ok(())
    }
}
fn take_fixed<const N: usize>(remaining: &mut &[u8]) -> Result<[u8; N], IdentityError> {
    let value = remaining.get(..N).ok_or(IdentityError::Malformed)?;
    let value = value.try_into().map_err(|_| IdentityError::Malformed)?;
    *remaining = &remaining[N..];
    Ok(value)
}

/// Check a FRESH durable contact snapshot and actual authenticated end-peer
/// (never a relay assertion), then verify the exact selected publisher bundle.
/// Pure check only: callers still serialize trust/freshness with later use.
pub fn verify_olm_binding(
    contact: Option<&crate::ContactTrust>,
    intended_device: [u8; 32],
    proof: &OlmBindingProof,
    expected: &OlmBindingData,
    authenticated_peer: Option<&PeerId>,
    now_ms: u64,
) -> Result<(), IdentityError> {
    let binding = contact.and_then(|c| c.pinned()).map(|p| p.binding());
    match crate::send_decision(contact, intended_device, binding, authenticated_peer) {
        crate::SendDecision::Hold(error) => return Err(error),
        crate::SendDecision::Allow => {}
    }
    proof.verify(
        intended_device,
        binding.ok_or(IdentityError::Unauthenticated)?,
        expected,
        now_ms,
    )
}
macro_rules! redacted_debug {
    ($($t:ty),*) => { $(impl fmt::Debug for $t {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(concat!(stringify!($t), "([REDACTED])"))
        }
    })* };
}
redacted_debug!(OlmBindingData, OlmBindingProof);
