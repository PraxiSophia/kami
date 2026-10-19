//! Named, deterministic semantic vectors. These are not wire/crypto fixtures.
//!
//! Exhaustion boundary vector: the existing owning-module unit test
//! `delivery::tests::attempt_counter_exhaustion_does_not_wrap_or_discard_the_message`
//! covers MAX -> AttemptsExhausted with complete snapshot preservation. The
//! public API cannot seed its private counter; do not add an unsafe/test-only
//! construction hook or pretend a short integration history reaches u64::MAX.
use kami_core::*;
use DeliveryEvent::{
    AuthorizationDenied, AuthorizationRestored, Cancel, IdentityChanged, IdentityReverified,
    StartAttempt, TransportAccepted,
};
use DeliveryPath::{Direct, Relay};
use DeliveryState::{
    AttemptFailed, BlockedAuthorization, BlockedIdentity, Canceled, DeviceDelivered, InFlight,
    Queued,
};
use TransitionError::{
    IdentityMismatch, InvalidReceipt, InvalidTransition, StaleAttempt, UnsolicitedReceipt,
};

type Projection = (
    DeliveryState,
    u64,
    Option<DeliveryPath>,
    Option<AttemptFailure>,
);
type Step = (DeliveryEvent, Result<Projection, TransitionError>);

fn facts() -> MessageFacts {
    MessageFacts::new(
        MessageId::from_u128(7),
        ConversationId::from_u128(8),
        DeviceId::from_bytes([1; 32]),
        DeviceId::from_bytes([2; 32]),
        SenderSequence::new(3),
        ContentBinding::from_bytes([4; 32]),
    )
}

fn receipt() -> DeliveryEvent {
    DeliveryEvent::Receipt(ReceiptEvidence::new(
        facts(),
        Some(facts().recipient()),
        true,
    ))
}

fn failed(attempt: u64, reason: AttemptFailure) -> DeliveryEvent {
    DeliveryEvent::AttemptFailed { attempt, reason }
}

fn projection(d: Delivery) -> Projection {
    (d.state(), d.attempts(), d.path(), d.last_failure())
}

fn run(name: &str, steps: &[Step]) -> Delivery {
    let original = facts();
    let mut delivery = Delivery::queued(original);
    assert_eq!(
        projection(delivery),
        (Queued, 0, None, None),
        "{name}: enqueue"
    );
    for (index, (event, expected)) in steps.iter().enumerate() {
        let snapshot = delivery;
        let result = delivery.reduce(*event);
        assert_eq!(delivery, snapshot, "{name}[{index}]: pure input");
        assert_eq!(
            result.map(projection),
            *expected,
            "{name}[{index}]: {event:?} from {snapshot:?}",
        );
        if let Ok(next) = result {
            delivery = next;
        } else {
            assert_eq!(delivery, snapshot, "{name}[{index}]: rejected snapshot");
        }
        assert_eq!(
            delivery.facts(),
            original,
            "{name}[{index}]: immutable facts"
        );
    }
    delivery
}

#[test]
fn golden_normal_delivery_requires_device_receipt() {
    let delivered = run(
        "normal delivery",
        &[
            (StartAttempt(Direct), Ok((InFlight, 1, Some(Direct), None))),
            (
                TransportAccepted { attempt: 1 },
                Ok((InFlight, 1, Some(Direct), None)),
            ),
            (receipt(), Ok((DeviceDelivered, 1, Some(Direct), None))),
            (receipt(), Ok((DeviceDelivered, 1, Some(Direct), None))),
            (Cancel, Err(InvalidTransition)),
        ],
    );
    assert_eq!(delivered.state(), DeviceDelivered);
}

#[test]
fn golden_direct_failure_then_relay_retry() {
    run(
        "direct failure to relay",
        &[
            (StartAttempt(Direct), Ok((InFlight, 1, Some(Direct), None))),
            (
                failed(1, AttemptFailure::ConnectionLost),
                Ok((
                    AttemptFailed,
                    1,
                    Some(Direct),
                    Some(AttemptFailure::ConnectionLost),
                )),
            ),
            (StartAttempt(Relay), Ok((InFlight, 2, Some(Relay), None))),
            (
                TransportAccepted { attempt: 2 },
                Ok((InFlight, 2, Some(Relay), None)),
            ),
            (receipt(), Ok((DeviceDelivered, 2, Some(Relay), None))),
        ],
    );
}

#[test]
fn golden_lost_ack_then_valid_logical_receipt() {
    run(
        "lost ACK then receipt",
        &[
            (StartAttempt(Direct), Ok((InFlight, 1, Some(Direct), None))),
            (
                TransportAccepted { attempt: 1 },
                Ok((InFlight, 1, Some(Direct), None)),
            ),
            (
                failed(1, AttemptFailure::AckLost),
                Ok((
                    AttemptFailed,
                    1,
                    Some(Direct),
                    Some(AttemptFailure::AckLost),
                )),
            ),
            (StartAttempt(Relay), Ok((InFlight, 2, Some(Relay), None))),
            (TransportAccepted { attempt: 1 }, Err(StaleAttempt)),
            // Receipt binds the logical message, not an attempt or the new path.
            (receipt(), Ok((DeviceDelivered, 2, Some(Relay), None))),
        ],
    );
}

#[test]
fn golden_identity_change_reverification_cannot_redirect() {
    run(
        "original identity only",
        &[
            (StartAttempt(Direct), Ok((InFlight, 1, Some(Direct), None))),
            (
                IdentityChanged,
                Ok((BlockedIdentity, 1, Some(Direct), None)),
            ),
            (StartAttempt(Relay), Err(InvalidTransition)),
            (IdentityReverified(facts().sender()), Err(IdentityMismatch)),
            (
                IdentityReverified(facts().recipient()),
                Ok((Queued, 1, Some(Direct), None)),
            ),
            (TransportAccepted { attempt: 1 }, Err(InvalidTransition)),
            (StartAttempt(Relay), Ok((InFlight, 2, Some(Relay), None))),
            (receipt(), Ok((DeviceDelivered, 2, Some(Relay), None))),
        ],
    );
}

#[test]
fn golden_authorization_and_identity_holds_restore_independently() {
    // State plus independent restoration probes expose both holds without
    // requiring a new public getter for the internal authorization flag.
    run(
        "restore authorization first",
        &[
            (StartAttempt(Direct), Ok((InFlight, 1, Some(Direct), None))),
            (
                AuthorizationDenied,
                Ok((BlockedAuthorization, 1, Some(Direct), None)),
            ),
            (StartAttempt(Relay), Err(InvalidTransition)),
            (
                IdentityChanged,
                Ok((BlockedIdentity, 1, Some(Direct), None)),
            ),
            (
                AuthorizationRestored,
                Ok((BlockedIdentity, 1, Some(Direct), None)),
            ),
            (StartAttempt(Relay), Err(InvalidTransition)),
            (
                IdentityReverified(facts().recipient()),
                Ok((Queued, 1, Some(Direct), None)),
            ),
            (StartAttempt(Relay), Ok((InFlight, 2, Some(Relay), None))),
        ],
    );
    run(
        "restore identity first",
        &[
            (IdentityChanged, Ok((BlockedIdentity, 0, None, None))),
            (AuthorizationDenied, Ok((BlockedIdentity, 0, None, None))),
            (
                IdentityReverified(facts().recipient()),
                Ok((BlockedAuthorization, 0, None, None)),
            ),
            (StartAttempt(Direct), Err(InvalidTransition)),
            (AuthorizationRestored, Ok((Queued, 0, None, None))),
            (StartAttempt(Direct), Ok((InFlight, 1, Some(Direct), None))),
        ],
    );
}

#[test]
fn golden_cancellation_stops_retry_not_a_valid_late_ack() {
    run(
        "cancel after attempt under both holds",
        &[
            (StartAttempt(Direct), Ok((InFlight, 1, Some(Direct), None))),
            (
                failed(1, AttemptFailure::Unavailable),
                Ok((
                    AttemptFailed,
                    1,
                    Some(Direct),
                    Some(AttemptFailure::Unavailable),
                )),
            ),
            (
                IdentityChanged,
                Ok((
                    BlockedIdentity,
                    1,
                    Some(Direct),
                    Some(AttemptFailure::Unavailable),
                )),
            ),
            (
                AuthorizationDenied,
                Ok((
                    BlockedIdentity,
                    1,
                    Some(Direct),
                    Some(AttemptFailure::Unavailable),
                )),
            ),
            (
                Cancel,
                Ok((Canceled, 1, Some(Direct), Some(AttemptFailure::Unavailable))),
            ),
            (StartAttempt(Relay), Err(InvalidTransition)),
            (AuthorizationRestored, Err(InvalidTransition)),
            (
                IdentityReverified(facts().recipient()),
                Err(InvalidTransition),
            ),
            (
                Cancel,
                Ok((Canceled, 1, Some(Direct), Some(AttemptFailure::Unavailable))),
            ),
            (
                receipt(),
                Ok((
                    DeviceDelivered,
                    1,
                    Some(Direct),
                    Some(AttemptFailure::Unavailable),
                )),
            ),
        ],
    );
    run(
        "cancel before attempt",
        &[
            (Cancel, Ok((Canceled, 0, None, None))),
            (StartAttempt(Direct), Err(InvalidTransition)),
            (receipt(), Err(UnsolicitedReceipt)),
        ],
    );
}

#[test]
fn golden_invalid_transitions_preserve_complete_prior_state() {
    run(
        "invalid events and stale attempts",
        &[
            (TransportAccepted { attempt: 0 }, Err(InvalidTransition)),
            (AuthorizationRestored, Err(InvalidTransition)),
            (receipt(), Err(UnsolicitedReceipt)),
            (StartAttempt(Direct), Ok((InFlight, 1, Some(Direct), None))),
            (StartAttempt(Relay), Err(InvalidTransition)),
            (TransportAccepted { attempt: 0 }, Err(StaleAttempt)),
            (failed(u64::MAX, AttemptFailure::AckLost), Err(StaleAttempt)),
            (
                DeliveryEvent::Receipt(ReceiptEvidence::new(facts(), None, true)),
                Err(InvalidReceipt),
            ),
            (
                failed(1, AttemptFailure::Unavailable),
                Ok((
                    AttemptFailed,
                    1,
                    Some(Direct),
                    Some(AttemptFailure::Unavailable),
                )),
            ),
            (TransportAccepted { attempt: 1 }, Err(InvalidTransition)),
            (StartAttempt(Relay), Ok((InFlight, 2, Some(Relay), None))),
            (failed(1, AttemptFailure::ConnectionLost), Err(StaleAttempt)),
        ],
    );
}
