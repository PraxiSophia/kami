use crate::{verify_peer_binding, DeviceFingerprint, IdentityError, Invitation, PeerBindingProof};
use libp2p_identity::PeerId;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ContactTrustState {
    Pending = 0,
    Verified = 1,
    Changed = 2,
    Blocked = 3,
}

/// Service-owned checked snapshot. Publish/use transitions only after durable CAS.
#[derive(Clone, Eq, PartialEq)]
pub struct ContactTrust {
    state: ContactTrustState,
    generation: u64,
    pinned: Option<Invitation>,
    candidate: Invitation,
}
impl ContactTrust {
    pub fn pending(candidate: Invitation) -> Self {
        Self {
            state: ContactTrustState::Pending,
            generation: 1,
            pinned: None,
            candidate,
        }
    }
    pub fn state(&self) -> ContactTrustState {
        self.state
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn pinned(&self) -> Option<&Invitation> {
        self.pinned.as_ref()
    }
    pub fn candidate(&self) -> &Invitation {
        &self.candidate
    }
    /// Trusted encrypted repository restoration ONLY, never an invitation/IPC API.
    /// Rechecks every signature, version, bound and state invariant on restart.
    pub fn from_persisted(
        state: ContactTrustState,
        generation: u64,
        pinned: Option<&[u8]>,
        candidate: &[u8],
    ) -> Result<Self, IdentityError> {
        if generation == 0 || generation > i64::MAX as u64 {
            return Err(IdentityError::Malformed);
        }
        let candidate = Invitation::parse_bounded(candidate)?;
        let pinned = pinned.map(Invitation::parse_bounded).transpose()?;
        match state {
            ContactTrustState::Pending if pinned.is_some() => return Err(IdentityError::Malformed),
            ContactTrustState::Verified if pinned.as_ref() != Some(&candidate) => {
                return Err(IdentityError::Malformed)
            }
            ContactTrustState::Changed if pinned.is_none() => return Err(IdentityError::Malformed),
            _ => {}
        }
        Ok(Self {
            state,
            generation,
            pinned,
            candidate,
        })
    }
    pub fn transition(&self, transition: TrustTransition) -> Result<Self, IdentityError> {
        let mut next = self.clone();
        match transition {
            TrustTransition::Observe(candidate) => {
                if next.state == ContactTrustState::Verified
                    && next.pinned.as_ref() != Some(&candidate)
                {
                    next.state = ContactTrustState::Changed;
                }
                // Blocking is sticky, including when a new candidate is imported.
                next.candidate = candidate;
            }
            TrustTransition::Block => next.state = ContactTrustState::Blocked,
            TrustTransition::Unblock => {
                if self.state != ContactTrustState::Blocked {
                    return Err(IdentityError::Conflict);
                }
                next.state = if self.pinned.is_some() {
                    ContactTrustState::Changed
                } else {
                    ContactTrustState::Pending
                };
            }
            TrustTransition::Verify(evidence) => {
                if self.state == ContactTrustState::Blocked {
                    return Err(IdentityError::Blocked);
                }
                if evidence.generation != self.generation || evidence.candidate != self.candidate {
                    return Err(IdentityError::Conflict);
                }
                next.pinned = Some(self.candidate.clone());
                next.state = ContactTrustState::Verified;
            }
        }
        next.generation = self
            .generation
            .checked_add(1)
            .filter(|v| *v <= i64::MAX as u64)
            .ok_or(IdentityError::Conflict)?;
        Ok(next)
    }
}
/// Only a trusted local affirmative user action following independent comparison
/// may construct this capability. Never construct from a network assertion.
/// Each endpoint performs its OWN action; this cannot prove the remote human did so.
pub struct VerificationEvidence {
    generation: u64,
    candidate: Invitation,
}
impl VerificationEvidence {
    pub fn independent_comparison(
        contact: &ContactTrust,
        compared: DeviceFingerprint,
        affirmative: bool,
    ) -> Result<Self, IdentityError> {
        if !affirmative || compared != contact.candidate.fingerprint() {
            return Err(IdentityError::VerificationRequired);
        }
        if contact.state == ContactTrustState::Blocked {
            return Err(IdentityError::Blocked);
        }
        Ok(Self {
            generation: contact.generation,
            candidate: contact.candidate.clone(),
        })
    }
}
pub enum TrustTransition {
    Observe(Invitation),
    Verify(VerificationEvidence),
    Block,
    Unblock,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SendDecision {
    Allow,
    Hold(IdentityError),
}
/// Check a FRESH durable snapshot for every enqueue/retry. `intended_device` is
/// the immutable original recipient key, never the new candidate's key.
/// This pure decision does not lock storage or implement a network sender.
pub fn send_decision(
    contact: Option<&ContactTrust>,
    intended_device: [u8; 32],
    proof: Option<&PeerBindingProof>,
    authenticated: Option<&PeerId>,
) -> SendDecision {
    let Some(contact) = contact else {
        return SendDecision::Hold(IdentityError::Unknown);
    };
    let reason = match contact.state {
        ContactTrustState::Pending => Some(IdentityError::VerificationRequired),
        ContactTrustState::Changed => Some(IdentityError::Changed),
        ContactTrustState::Blocked => Some(IdentityError::Blocked),
        ContactTrustState::Verified => None,
    };
    if let Some(reason) = reason {
        return SendDecision::Hold(reason);
    }
    let Some(pin) = &contact.pinned else {
        return SendDecision::Hold(IdentityError::Malformed);
    };
    if intended_device != *pin.device_key() {
        return SendDecision::Hold(IdentityError::Changed);
    }
    let Some(proof) = proof else {
        return SendDecision::Hold(IdentityError::Unauthenticated);
    };
    if proof.peer_id() != pin.binding().peer_id() {
        return SendDecision::Hold(IdentityError::Changed);
    }
    match verify_peer_binding(intended_device, proof, authenticated) {
        Ok(()) => SendDecision::Allow,
        Err(error) => SendDecision::Hold(error),
    }
}
/// Trusted service capability, not IPC. Implement using the existing encrypted
/// storage worker's neutral row/load/CAS API; never a sidecar or message payload.
/// None means insert-if-absent at generation 1; Some(n) requires generation n+1.
/// Conflicts/errors MUST reload and recompute, never blind retry a stale snapshot.
pub trait ContactTrustRepository {
    fn load(&self, contact: [u8; 32]) -> Result<Option<ContactTrust>, IdentityError>;
    fn compare_and_swap(
        &self,
        contact: [u8; 32],
        expected: Option<u64>,
        next: &ContactTrust,
    ) -> Result<(), IdentityError>;
}
macro_rules! redacted_debug {
    ($($t:ty),*) => { $(impl fmt::Debug for $t {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(concat!(stringify!($t), "([REDACTED])")) }
    })* };
}
redacted_debug!(ContactTrust, VerificationEvidence, TrustTransition);
