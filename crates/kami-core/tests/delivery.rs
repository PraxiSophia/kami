use kami_core::*;

fn message() -> MessageFacts {
    MessageFacts::new(
        MessageId::from_u128(7),
        ConversationId::from_u128(8),
        DeviceId::from_bytes([1; 32]),
        DeviceId::from_bytes([2; 32]),
        SenderSequence::new(3),
        ContentBinding::from_bytes([4; 32]),
    )
}

fn receipt(facts: MessageFacts) -> ReceiptEvidence {
    ReceiptEvidence::new(facts, Some(facts.recipient()), true)
}

#[test]
fn ack_loss_retries_the_same_message_on_another_path() {
    let queued = Delivery::queued(message());
    let direct = queued
        .reduce(DeliveryEvent::StartAttempt(DeliveryPath::Direct))
        .unwrap();
    assert_eq!(direct.state(), DeliveryState::InFlight);
    assert_eq!(direct.attempts(), 1);
    let written = direct
        .reduce(DeliveryEvent::TransportAccepted { attempt: 1 })
        .unwrap();
    assert_eq!(written, direct); // A socket write is not device delivery.
    let failed = written
        .reduce(DeliveryEvent::AttemptFailed {
            attempt: 1,
            reason: AttemptFailure::AckLost,
        })
        .unwrap();
    assert_eq!(failed.state(), DeliveryState::AttemptFailed);
    let relay = failed
        .reduce(DeliveryEvent::StartAttempt(DeliveryPath::Relay))
        .unwrap();
    assert_eq!(relay.facts(), queued.facts());
    assert_eq!(relay.attempts(), 2);
    assert_eq!(relay.path(), Some(DeliveryPath::Relay));
    assert_eq!(queued.state(), DeliveryState::Queued); // Reducer is pure.
    assert_eq!(
        failed.reduce(DeliveryEvent::StartAttempt(DeliveryPath::Relay)),
        Ok(relay)
    );
}

#[test]
fn only_a_matching_authenticated_durable_receipt_delivers() {
    let queued = Delivery::queued(message());
    let active = queued
        .reduce(DeliveryEvent::StartAttempt(DeliveryPath::Direct))
        .unwrap();
    let valid = DeliveryEvent::Receipt(receipt(message()));
    assert_eq!(
        queued.reduce(valid),
        Err(TransitionError::UnsolicitedReceipt)
    );
    let delivered = active.reduce(valid).unwrap();
    assert_eq!(delivered.state(), DeliveryState::DeviceDelivered);
    assert_eq!(delivered.reduce(valid), Ok(delivered));
    for evidence in [
        ReceiptEvidence::new(message(), None, true),
        ReceiptEvidence::new(message(), Some(message().sender()), true),
        ReceiptEvidence::new(message(), Some(message().recipient()), false),
    ] {
        assert_eq!(
            active.reduce(DeliveryEvent::Receipt(evidence)),
            Err(TransitionError::InvalidReceipt)
        );
        assert_eq!(
            delivered.reduce(DeliveryEvent::Receipt(evidence)),
            Err(TransitionError::InvalidReceipt)
        );
    }
    let original = message();
    let mismatches = [
        MessageFacts::new(
            MessageId::from_u128(99),
            original.conversation_id(),
            original.sender(),
            original.recipient(),
            original.sequence(),
            original.content_binding(),
        ),
        MessageFacts::new(
            original.message_id(),
            ConversationId::from_u128(99),
            original.sender(),
            original.recipient(),
            original.sequence(),
            original.content_binding(),
        ),
        MessageFacts::new(
            original.message_id(),
            original.conversation_id(),
            DeviceId::from_bytes([99; 32]),
            original.recipient(),
            original.sequence(),
            original.content_binding(),
        ),
        MessageFacts::new(
            original.message_id(),
            original.conversation_id(),
            original.sender(),
            DeviceId::from_bytes([99; 32]),
            original.sequence(),
            original.content_binding(),
        ),
        MessageFacts::new(
            original.message_id(),
            original.conversation_id(),
            original.sender(),
            original.recipient(),
            SenderSequence::new(99),
            original.content_binding(),
        ),
        MessageFacts::new(
            original.message_id(),
            original.conversation_id(),
            original.sender(),
            original.recipient(),
            original.sequence(),
            ContentBinding::from_bytes([99; 32]),
        ),
    ];
    for mismatch in mismatches {
        assert_eq!(
            active.reduce(DeliveryEvent::Receipt(receipt(mismatch))),
            Err(TransitionError::InvalidReceipt)
        );
    }
    assert_eq!(active.state(), DeliveryState::InFlight);
}

#[test]
fn identity_change_holds_attempts_without_redirecting() {
    let active = Delivery::queued(message())
        .reduce(DeliveryEvent::StartAttempt(DeliveryPath::Direct))
        .unwrap();
    let blocked = active.reduce(DeliveryEvent::IdentityChanged).unwrap();
    assert_eq!(blocked.state(), DeliveryState::BlockedIdentity);
    assert_eq!(
        blocked.reduce(DeliveryEvent::StartAttempt(DeliveryPath::Relay)),
        Err(TransitionError::InvalidTransition)
    );
    assert_eq!(
        blocked.reduce(DeliveryEvent::IdentityReverified(DeviceId::from_bytes(
            [99; 32]
        ))),
        Err(TransitionError::IdentityMismatch)
    );
    let resumed = blocked
        .reduce(DeliveryEvent::IdentityReverified(message().recipient()))
        .unwrap();
    assert_eq!(resumed.state(), DeliveryState::Queued);
    assert_eq!(resumed.facts(), message());
    assert!(resumed
        .reduce(DeliveryEvent::TransportAccepted { attempt: 1 })
        .is_err());
    let retried = resumed
        .reduce(DeliveryEvent::StartAttempt(DeliveryPath::Relay))
        .unwrap();
    assert_eq!(retried.attempts(), 2);
    // A late ACK from the ORIGINAL expected device remains valid while held.
    assert_eq!(
        blocked
            .reduce(DeliveryEvent::Receipt(receipt(message())))
            .unwrap()
            .state(),
        DeliveryState::DeviceDelivered
    );
}

#[test]
fn authorization_restore_cannot_clear_an_identity_hold() {
    let queued = Delivery::queued(message());
    let active = queued
        .reduce(DeliveryEvent::StartAttempt(DeliveryPath::Direct))
        .unwrap();
    let denied = active.reduce(DeliveryEvent::AuthorizationDenied).unwrap();
    assert_eq!(denied.state(), DeliveryState::BlockedAuthorization);
    assert!(denied
        .reduce(DeliveryEvent::StartAttempt(DeliveryPath::Direct))
        .is_err());
    let both = denied.reduce(DeliveryEvent::IdentityChanged).unwrap();
    assert_eq!(both.state(), DeliveryState::BlockedIdentity);
    let identity_only = both.reduce(DeliveryEvent::AuthorizationRestored).unwrap();
    assert_eq!(identity_only.state(), DeliveryState::BlockedIdentity);
    let authorization_only = both
        .reduce(DeliveryEvent::IdentityReverified(message().recipient()))
        .unwrap();
    assert_eq!(
        authorization_only.state(),
        DeliveryState::BlockedAuthorization
    );
    assert_eq!(
        authorization_only
            .reduce(DeliveryEvent::AuthorizationRestored)
            .unwrap()
            .state(),
        DeliveryState::Queued
    );
    assert!(queued.reduce(DeliveryEvent::AuthorizationRestored).is_err());
}

#[test]
fn cancellation_stops_retries_but_cannot_recall_remote_delivery() {
    let queued = Delivery::queued(message());
    let active = queued
        .reduce(DeliveryEvent::StartAttempt(DeliveryPath::Direct))
        .unwrap();
    let failed = active
        .reduce(DeliveryEvent::AttemptFailed {
            attempt: 1,
            reason: AttemptFailure::Unavailable,
        })
        .unwrap();
    let blocked = active.reduce(DeliveryEvent::IdentityChanged).unwrap();
    let denied = active.reduce(DeliveryEvent::AuthorizationDenied).unwrap();
    for pending in [queued, active, failed, blocked, denied] {
        let canceled = pending.reduce(DeliveryEvent::Cancel).unwrap();
        assert_eq!(canceled.state(), DeliveryState::Canceled);
        assert_eq!(canceled.reduce(DeliveryEvent::Cancel), Ok(canceled));
        for event in [
            DeliveryEvent::StartAttempt(DeliveryPath::Relay),
            DeliveryEvent::IdentityChanged,
            DeliveryEvent::AuthorizationDenied,
            DeliveryEvent::AuthorizationRestored,
            DeliveryEvent::IdentityReverified(message().recipient()),
        ] {
            assert_eq!(
                canceled.reduce(event),
                Err(TransitionError::InvalidTransition)
            );
        }
        let late = canceled.reduce(DeliveryEvent::Receipt(receipt(message())));
        if pending.attempts() == 0 {
            assert_eq!(late, Err(TransitionError::UnsolicitedReceipt));
        } else {
            let delivered = late.unwrap();
            assert_eq!(delivered.state(), DeliveryState::DeviceDelivered);
            assert_eq!(
                delivered.reduce(DeliveryEvent::Cancel),
                Err(TransitionError::InvalidTransition)
            );
        }
    }
}

#[test]
fn transition_matrix_rejects_invalid_events_without_state_regression() {
    use DeliveryEvent::*;
    use DeliveryState::{
        BlockedAuthorization, BlockedIdentity, Canceled, DeviceDelivered, InFlight, Queued,
    };
    let queued = Delivery::queued(message());
    let active = queued.reduce(StartAttempt(DeliveryPath::Direct)).unwrap();
    let failed = active
        .reduce(AttemptFailed {
            attempt: 1,
            reason: AttemptFailure::ConnectionLost,
        })
        .unwrap();
    assert_eq!(queued.path(), None);
    assert_eq!(queued.last_failure(), None);
    assert_eq!(failed.last_failure(), Some(AttemptFailure::ConnectionLost));
    let blocked = active.reduce(IdentityChanged).unwrap();
    let denied = active.reduce(AuthorizationDenied).unwrap();
    let both = blocked.reduce(AuthorizationDenied).unwrap();
    let canceled = active.reduce(Cancel).unwrap();
    let delivered = active.reduce(Receipt(receipt(message()))).unwrap();
    let events = [
        StartAttempt(DeliveryPath::Relay),
        TransportAccepted { attempt: 1 },
        AttemptFailed {
            attempt: 1,
            reason: AttemptFailure::ConnectionLost,
        },
        Receipt(receipt(message())),
        IdentityChanged,
        IdentityReverified(message().recipient()),
        AuthorizationDenied,
        AuthorizationRestored,
        Cancel,
    ];
    // Each cell explicitly declares success + projected state, or rejection.
    let rows = [
        (
            queued,
            [
                Some(InFlight),
                None,
                None,
                None,
                Some(BlockedIdentity),
                None,
                Some(BlockedAuthorization),
                None,
                Some(Canceled),
            ],
        ),
        (
            active,
            [
                None,
                Some(InFlight),
                Some(DeliveryState::AttemptFailed),
                Some(DeviceDelivered),
                Some(BlockedIdentity),
                None,
                Some(BlockedAuthorization),
                None,
                Some(Canceled),
            ],
        ),
        (
            failed,
            [
                Some(InFlight),
                None,
                None,
                Some(DeviceDelivered),
                Some(BlockedIdentity),
                None,
                Some(BlockedAuthorization),
                None,
                Some(Canceled),
            ],
        ),
        (
            blocked,
            [
                None,
                None,
                None,
                Some(DeviceDelivered),
                Some(BlockedIdentity),
                Some(Queued),
                Some(BlockedIdentity),
                None,
                Some(Canceled),
            ],
        ),
        (
            denied,
            [
                None,
                None,
                None,
                Some(DeviceDelivered),
                Some(BlockedIdentity),
                None,
                Some(BlockedAuthorization),
                Some(Queued),
                Some(Canceled),
            ],
        ),
        (
            both,
            [
                None,
                None,
                None,
                Some(DeviceDelivered),
                Some(BlockedIdentity),
                Some(BlockedAuthorization),
                Some(BlockedIdentity),
                Some(BlockedIdentity),
                Some(Canceled),
            ],
        ),
        (
            canceled,
            [
                None,
                None,
                None,
                Some(DeviceDelivered),
                None,
                None,
                None,
                None,
                Some(Canceled),
            ],
        ),
        (
            delivered,
            [
                None,
                None,
                None,
                Some(DeviceDelivered),
                None,
                None,
                None,
                None,
                None,
            ],
        ),
    ];
    for (projection, expected) in rows {
        for (event, state) in events.into_iter().zip(expected) {
            let snapshot = projection;
            let result = projection.reduce(event);
            assert_eq!(projection, snapshot);
            assert_eq!(result, projection.reduce(event));
            assert_eq!(
                result.as_ref().ok().map(|next| next.state()),
                state,
                "{projection:?} + {event:?}"
            );
            if let Ok(next) = result {
                assert_eq!(next.facts(), message());
            }
        }
    }
    let retry = failed.reduce(StartAttempt(DeliveryPath::Relay)).unwrap();
    for attempt in [0, 1, 3, u64::MAX] {
        assert_eq!(
            retry.reduce(TransportAccepted { attempt }),
            Err(TransitionError::StaleAttempt)
        );
        assert_eq!(
            retry.reduce(AttemptFailed {
                attempt,
                reason: AttemptFailure::AckLost
            }),
            Err(TransitionError::StaleAttempt)
        );
    }
    // A delayed ACK is tied to the logical message, not to a transport attempt.
    for pending in [failed, retry, denied] {
        assert_eq!(
            pending.reduce(Receipt(receipt(message()))).unwrap().state(),
            DeviceDelivered
        );
    }
}

#[test]
fn committed_event_replay_preserves_attempts_and_identity_holds() {
    let committed = [
        DeliveryEvent::StartAttempt(DeliveryPath::Direct),
        DeliveryEvent::AttemptFailed {
            attempt: 1,
            reason: AttemptFailure::AckLost,
        },
        DeliveryEvent::StartAttempt(DeliveryPath::Relay),
        DeliveryEvent::IdentityChanged,
        DeliveryEvent::AuthorizationDenied,
    ];
    let apply = |events: &[DeliveryEvent]| {
        events
            .iter()
            .try_fold(Delivery::queued(message()), |state, event| {
                state.reduce(*event)
            })
            .unwrap()
    };
    let live = apply(&committed);
    let reconstructed = apply(&committed);
    assert_eq!(live, reconstructed);
    assert_eq!(reconstructed.state(), DeliveryState::BlockedIdentity);
    assert_eq!(reconstructed.attempts(), 2);
    assert_eq!(reconstructed.facts(), message());
    let reverified = reconstructed
        .reduce(DeliveryEvent::IdentityReverified(message().recipient()))
        .unwrap();
    assert_eq!(reverified.state(), DeliveryState::BlockedAuthorization);
    let next = reverified
        .reduce(DeliveryEvent::AuthorizationRestored)
        .unwrap()
        .reduce(DeliveryEvent::StartAttempt(DeliveryPath::Direct))
        .unwrap();
    assert_eq!(next.attempts(), 3);
    assert_eq!(
        next.reduce(DeliveryEvent::TransportAccepted { attempt: 2 }),
        Err(TransitionError::StaleAttempt)
    );
}
