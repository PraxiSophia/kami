//! Pure delivery projection, attempt events, and semantic receipt binding.
//! Receipt evidence is trusted adapter input, not cryptographic verification.

use crate::ids::DeviceId;
use crate::message::MessageFacts;

/// A transport choice is attempt metadata, never part of logical identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryPath {
    Direct,
    Relay,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryState {
    Queued,
    InFlight,
    AttemptFailed,
    DeviceDelivered,
    BlockedIdentity,
    BlockedAuthorization,
    Canceled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttemptFailure {
    AckLost,
    ConnectionLost,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryEvent {
    StartAttempt(DeliveryPath),
    TransportAccepted {
        attempt: u64,
    },
    AttemptFailed {
        attempt: u64,
        reason: AttemptFailure,
    },
    Receipt(ReceiptEvidence),
    IdentityChanged,
    /// Explicit independent reverification of the ORIGINAL expected identity.
    /// Adopting replacement keys cannot redirect this immutable queued message.
    IdentityReverified(DeviceId),
    AuthorizationDenied,
    AuthorizationRestored,
    /// Stops future attempts, not remote persistence or an already-sent ACK.
    Cancel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransitionError {
    InvalidTransition,
    StaleAttempt,
    AttemptsExhausted,
    InvalidReceipt,
    UnsolicitedReceipt,
    IdentityMismatch,
}

/// Facts supplied by a trusted protocol adapter after authenticating a receipt.
/// `authenticated_device` must come from verified end-to-end authentication of
/// ALL receipt fields, not from an untrusted claimed device or relay identity.
/// `durably_stored` is the authenticated device-delivered assertion, not a
/// socket-write result. This crate checks semantic binding only; it cannot
/// verify signatures/MACs or independently establish remote persistence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReceiptEvidence {
    facts: MessageFacts,
    authenticated_device: Option<DeviceId>,
    durably_stored: bool,
}

impl ReceiptEvidence {
    pub const fn new(
        facts: MessageFacts,
        authenticated_device: Option<DeviceId>,
        durably_stored: bool,
    ) -> Self {
        Self {
            facts,
            authenticated_device,
            durably_stored,
        }
    }
}

/// Pure outbox projection. This is not a database or scheduler. Construct only
/// after verified-recipient validation and durable enqueue; commit a returned
/// projection before acting on it. Never reconstruct an existing message using
/// `queued`: replay its committed events instead.
/// Retain attempt counters across restart to reject delayed transport events.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Delivery {
    facts: MessageFacts,
    state: DeliveryState,
    attempts: u64,
    path: Option<DeliveryPath>,
    last_failure: Option<AttemptFailure>,
    authorization_blocked: bool,
}

impl Delivery {
    pub const fn queued(facts: MessageFacts) -> Self {
        Self {
            facts,
            state: DeliveryState::Queued,
            attempts: 0,
            path: None,
            last_failure: None,
            authorization_blocked: false,
        }
    }
    pub const fn facts(self) -> MessageFacts {
        self.facts
    }
    pub const fn state(self) -> DeliveryState {
        if self.authorization_blocked
            && !matches!(
                self.state,
                DeliveryState::BlockedIdentity
                    | DeliveryState::DeviceDelivered
                    | DeliveryState::Canceled
            )
        {
            DeliveryState::BlockedAuthorization
        } else {
            self.state
        }
    }
    pub const fn attempts(self) -> u64 {
        self.attempts
    }
    /// Most recently selected path, not necessarily an active connection.
    pub const fn path(self) -> Option<DeliveryPath> {
        self.path
    }
    /// Most recent attempt failure, cleared when another attempt begins.
    pub const fn last_failure(self) -> Option<AttemptFailure> {
        self.last_failure
    }

    /// Deterministic transition: no clock, randomness, I/O, or mutation on error.
    pub fn reduce(&self, event: DeliveryEvent) -> Result<Self, TransitionError> {
        let mut next = *self;
        match event {
            DeliveryEvent::Cancel => {
                if self.state == DeliveryState::DeviceDelivered {
                    return Err(TransitionError::InvalidTransition);
                }
                next.state = DeliveryState::Canceled;
            }
            DeliveryEvent::AuthorizationDenied => {
                if matches!(
                    self.state,
                    DeliveryState::DeviceDelivered | DeliveryState::Canceled
                ) {
                    return Err(TransitionError::InvalidTransition);
                }
                next.authorization_blocked = true;
                if self.state == DeliveryState::InFlight {
                    next.state = DeliveryState::Queued;
                }
            }
            DeliveryEvent::AuthorizationRestored => {
                if !self.authorization_blocked
                    || matches!(
                        self.state,
                        DeliveryState::DeviceDelivered | DeliveryState::Canceled
                    )
                {
                    return Err(TransitionError::InvalidTransition);
                }
                next.authorization_blocked = false;
            }
            DeliveryEvent::IdentityChanged => {
                if matches!(
                    self.state,
                    DeliveryState::DeviceDelivered | DeliveryState::Canceled
                ) {
                    return Err(TransitionError::InvalidTransition);
                }
                next.state = DeliveryState::BlockedIdentity;
            }
            DeliveryEvent::IdentityReverified(device) => {
                if self.state != DeliveryState::BlockedIdentity {
                    return Err(TransitionError::InvalidTransition);
                }
                if device != self.facts.recipient() {
                    return Err(TransitionError::IdentityMismatch);
                }
                next.state = DeliveryState::Queued;
            }
            DeliveryEvent::Receipt(receipt) => {
                if receipt.facts != self.facts
                    || receipt.authenticated_device != Some(self.facts.recipient())
                    || !receipt.durably_stored
                {
                    return Err(TransitionError::InvalidReceipt);
                }
                if self.attempts == 0 {
                    return Err(TransitionError::UnsolicitedReceipt);
                }
                // Late ACKs from the originally expected recipient are valid
                // even after cancel/hold: cancellation cannot recall a send.
                next.state = DeliveryState::DeviceDelivered;
            }
            DeliveryEvent::StartAttempt(path) => {
                if !matches!(
                    self.state(),
                    DeliveryState::Queued | DeliveryState::AttemptFailed
                ) {
                    return Err(TransitionError::InvalidTransition);
                }
                next.attempts = self
                    .attempts
                    .checked_add(1)
                    .ok_or(TransitionError::AttemptsExhausted)?;
                next.state = DeliveryState::InFlight;
                next.path = Some(path);
                next.last_failure = None;
            }
            DeliveryEvent::TransportAccepted { attempt }
            | DeliveryEvent::AttemptFailed { attempt, .. } => {
                if self.state() != DeliveryState::InFlight {
                    return Err(TransitionError::InvalidTransition);
                }
                if attempt != self.attempts {
                    return Err(TransitionError::StaleAttempt);
                }
                if let DeliveryEvent::AttemptFailed { reason, .. } = event {
                    next.state = DeliveryState::AttemptFailed;
                    next.last_failure = Some(reason);
                }
            }
        }
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{ConversationId, MessageId, SenderSequence};
    use crate::message::ContentBinding;

    #[test]
    fn attempt_counter_exhaustion_does_not_wrap_or_discard_the_message() {
        let facts = MessageFacts::new(
            MessageId::from_u128(7),
            ConversationId::from_u128(8),
            DeviceId::from_bytes([1; 32]),
            DeviceId::from_bytes([2; 32]),
            SenderSequence::new(3),
            ContentBinding::from_bytes([4; 32]),
        );
        // Exercise an otherwise unreachable-in-a-short-test durable counter.
        let exhausted = Delivery {
            attempts: u64::MAX,
            ..Delivery::queued(facts)
        };
        let snapshot = exhausted;
        assert_eq!(
            exhausted.reduce(DeliveryEvent::StartAttempt(DeliveryPath::Direct)),
            Err(TransitionError::AttemptsExhausted)
        );
        assert_eq!(exhausted, snapshot);
        assert_eq!(exhausted.facts(), facts);
        assert_eq!(exhausted.state(), DeliveryState::Queued);
    }
}
