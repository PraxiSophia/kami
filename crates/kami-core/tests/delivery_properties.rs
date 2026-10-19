//! Bounded semantic histories, not a second transition matrix. Receipt inputs
//! represent trusted adapter evidence; these tests do not establish crypto or I/O.
use kami_core::*;
use proptest::prelude::*;

fn facts() -> impl Strategy<Value = MessageFacts> {
    (
        1u128..1000,
        1u128..1000,
        1u8..64,
        1u64..1000,
        any::<[u8; 32]>(),
    )
        .prop_map(|(id, conversation, sender, sequence, content)| {
            MessageFacts::new(
                MessageId::from_u128(id),
                ConversationId::from_u128(conversation),
                DeviceId::from_bytes([sender; 32]),
                DeviceId::from_bytes([sender + 64; 32]),
                SenderSequence::new(sequence),
                ContentBinding::from_bytes(content),
            )
        })
}

fn path() -> impl Strategy<Value = DeliveryPath> {
    prop_oneof![Just(DeliveryPath::Direct), Just(DeliveryPath::Relay)]
}

fn failure() -> impl Strategy<Value = AttemptFailure> {
    prop_oneof![
        Just(AttemptFailure::AckLost),
        Just(AttemptFailure::ConnectionLost),
        Just(AttemptFailure::Unavailable),
    ]
}

// 0 is matching; 1..=6 each alter exactly one immutable fact; 7..=9
// retain the facts but omit authentication, use a wrong device, or deny durability.
fn evidence(f: MessageFacts, variant: u8) -> ReceiptEvidence {
    let changed = MessageFacts::new(
        if variant == 1 {
            MessageId::from_u128(2000)
        } else {
            f.message_id()
        },
        if variant == 2 {
            ConversationId::from_u128(2000)
        } else {
            f.conversation_id()
        },
        if variant == 3 {
            DeviceId::from_bytes([255; 32])
        } else {
            f.sender()
        },
        if variant == 4 {
            DeviceId::from_bytes([255; 32])
        } else {
            f.recipient()
        },
        if variant == 5 {
            SenderSequence::new(2000)
        } else {
            f.sequence()
        },
        if variant == 6 {
            let mut bytes = *f.content_binding().as_bytes();
            bytes[0] ^= 1;
            ContentBinding::from_bytes(bytes)
        } else {
            f.content_binding()
        },
    );
    let device = match variant {
        7 => None,
        8 => Some(f.sender()),
        _ => Some(f.recipient()),
    };
    ReceiptEvidence::new(changed, device, variant != 9)
}

#[derive(Clone, Debug)]
enum Op {
    Start(DeliveryPath),
    Accepted(u8),
    Failed(u8, AttemptFailure),
    Receipt(u8),
    IdentityChanged,
    Reverified(bool),
    Denied,
    Restored,
    Cancel,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        3 => path().prop_map(Op::Start),
        2 => (0u8..4).prop_map(Op::Accepted),
        2 => (0u8..4, failure()).prop_map(|(a, f)| Op::Failed(a, f)),
        2 => (0u8..10).prop_map(Op::Receipt),
        1 => Just(Op::IdentityChanged),
        1 => any::<bool>().prop_map(Op::Reverified),
        1 => Just(Op::Denied),
        1 => Just(Op::Restored),
        1 => Just(Op::Cancel),
    ]
}

impl Op {
    fn event(&self, delivery: Delivery) -> DeliveryEvent {
        let attempt = |selector| match selector {
            0 => 0,
            1 => delivery.attempts(),
            2 => delivery.attempts().saturating_sub(1),
            _ => delivery.attempts() + 1, // Bounded histories cannot approach MAX.
        };
        match *self {
            Self::Start(p) => DeliveryEvent::StartAttempt(p),
            Self::Accepted(a) => DeliveryEvent::TransportAccepted {
                attempt: attempt(a),
            },
            Self::Failed(a, reason) => DeliveryEvent::AttemptFailed {
                attempt: attempt(a),
                reason,
            },
            Self::Receipt(v) => DeliveryEvent::Receipt(evidence(delivery.facts(), v)),
            Self::IdentityChanged => DeliveryEvent::IdentityChanged,
            Self::Reverified(correct) => DeliveryEvent::IdentityReverified(if correct {
                delivery.facts().recipient()
            } else {
                delivery.facts().sender()
            }),
            Self::Denied => DeliveryEvent::AuthorizationDenied,
            Self::Restored => DeliveryEvent::AuthorizationRestored,
            Self::Cancel => DeliveryEvent::Cancel,
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn bounded_histories_preserve_facts_counters_and_rejected_snapshots(
        f in facts(), history in prop::collection::vec(op(), 0..25),
    ) {
        let mut delivery = Delivery::queued(f);
        let mut committed = Vec::new();
        for operation in history {
            let snapshot = delivery;
            let event = operation.event(delivery);
            let result = delivery.reduce(event);
            prop_assert_eq!(delivery, snapshot);
            prop_assert_eq!(result, delivery.reduce(event));
            match result {
                Err(_) => prop_assert_eq!(delivery, snapshot),
                Ok(next) => {
                    prop_assert_eq!(next.facts(), f);
                    prop_assert!(next.attempts() >= snapshot.attempts());
                    let increment = u64::from(matches!(event, DeliveryEvent::StartAttempt(_)));
                    prop_assert_eq!(next.attempts(), snapshot.attempts() + increment);
                    if let DeliveryEvent::TransportAccepted { .. } = event {
                        prop_assert_eq!(next, snapshot);
                        prop_assert_eq!(next.state(), DeliveryState::InFlight);
                    }
                    if next.state() == DeliveryState::DeviceDelivered
                        && snapshot.state() != DeliveryState::DeviceDelivered {
                        prop_assert!(matches!(operation, Op::Receipt(0)));
                        prop_assert!(snapshot.attempts() > 0);
                    }
                    committed.push(event);
                    delivery = next;
                }
            }
            // Replay only committed events, as a storage adapter must do.
            let replay = committed.iter().try_fold(Delivery::queued(f), |d, e| d.reduce(*e));
            prop_assert_eq!(replay, Ok(delivery));
        }
    }

    #[test]
    fn retry_histories_reject_stale_attempts_without_losing_metadata(
        f in facts(), attempts in prop::collection::vec((path(), failure()), 1..25),
    ) {
        let mut delivery = Delivery::queued(f);
        for (index, (p, reason)) in attempts.into_iter().enumerate() {
            delivery = delivery.reduce(DeliveryEvent::StartAttempt(p)).unwrap();
            let number = index as u64 + 1;
            prop_assert_eq!(delivery.attempts(), number);
            prop_assert_eq!(delivery.path(), Some(p));
            prop_assert_eq!(delivery.last_failure(), None);
            let snapshot = delivery;
            for stale in [0, number - 1, number + 1, u64::MAX] {
                for event in [
                    DeliveryEvent::TransportAccepted { attempt: stale },
                    DeliveryEvent::AttemptFailed { attempt: stale, reason },
                ] {
                    prop_assert_eq!(delivery.reduce(event), Err(TransitionError::StaleAttempt));
                    prop_assert_eq!(delivery, snapshot);
                }
            }
            prop_assert_eq!(delivery.reduce(DeliveryEvent::TransportAccepted { attempt: number }), Ok(snapshot));
            delivery = delivery.reduce(DeliveryEvent::AttemptFailed { attempt: number, reason }).unwrap();
            prop_assert_eq!(delivery.facts(), f);
            prop_assert_eq!(delivery.state(), DeliveryState::AttemptFailed);
            prop_assert_eq!(delivery.last_failure(), Some(reason));
        }
    }

    #[test]
    fn receipt_binding_is_required_after_generated_retries(
        f in facts(), paths in prop::collection::vec(path(), 0..25), invalid in 1u8..10,
    ) {
        let mut delivery = Delivery::queued(f);
        for p in paths {
            delivery = delivery.reduce(DeliveryEvent::StartAttempt(p)).unwrap();
            delivery = delivery.reduce(DeliveryEvent::AttemptFailed {
                attempt: delivery.attempts(), reason: AttemptFailure::AckLost,
            }).unwrap();
        }
        let snapshot = delivery;
        prop_assert_eq!(delivery.reduce(DeliveryEvent::Receipt(evidence(f, invalid))), Err(TransitionError::InvalidReceipt));
        prop_assert_eq!(delivery, snapshot);
        let result = delivery.reduce(DeliveryEvent::Receipt(evidence(f, 0)));
        if delivery.attempts() == 0 {
            prop_assert_eq!(result, Err(TransitionError::UnsolicitedReceipt));
        } else {
            let delivered = result.unwrap();
            prop_assert_eq!(delivered.state(), DeliveryState::DeviceDelivered);
            prop_assert_eq!(delivered.facts(), f);
            prop_assert_eq!(delivered.attempts(), delivery.attempts());
            prop_assert_eq!(delivered.path(), delivery.path());
            prop_assert_eq!(delivered.last_failure(), delivery.last_failure());
        }
    }

    #[test]
    fn independent_holds_survive_restoration_and_cannot_redirect(
        f in facts(), p in path(), identity_first in any::<bool>(), restore_identity_first in any::<bool>(),
    ) {
        let active = Delivery::queued(f).reduce(DeliveryEvent::StartAttempt(p)).unwrap();
        let events = if identity_first {
            [DeliveryEvent::IdentityChanged, DeliveryEvent::AuthorizationDenied]
        } else {
            [DeliveryEvent::AuthorizationDenied, DeliveryEvent::IdentityChanged]
        };
        let both = events.into_iter().try_fold(active, |d, e| d.reduce(e)).unwrap();
        prop_assert_eq!(both.state(), DeliveryState::BlockedIdentity);
        prop_assert_eq!(both.reduce(DeliveryEvent::IdentityReverified(f.sender())), Err(TransitionError::IdentityMismatch));
        prop_assert_eq!(both.facts().recipient(), f.recipient());
        let (first, second, remaining) = if restore_identity_first {
            (DeliveryEvent::IdentityReverified(f.recipient()), DeliveryEvent::AuthorizationRestored, DeliveryState::BlockedAuthorization)
        } else {
            (DeliveryEvent::AuthorizationRestored, DeliveryEvent::IdentityReverified(f.recipient()), DeliveryState::BlockedIdentity)
        };
        let held = both.reduce(first).unwrap();
        for blocked in [both, held] {
            prop_assert_eq!(blocked.reduce(DeliveryEvent::StartAttempt(p)), Err(TransitionError::InvalidTransition));
            prop_assert_eq!(blocked.facts(), f);
            prop_assert_eq!(blocked.attempts(), 1);
            prop_assert_eq!(blocked.path(), Some(p));
        }
        prop_assert_eq!(held.state(), remaining);
        let restored = held.reduce(second).unwrap();
        prop_assert_eq!(restored.state(), DeliveryState::Queued);
        let retry = restored.reduce(DeliveryEvent::StartAttempt(p)).unwrap();
        prop_assert_eq!(retry.attempts(), 2);
        prop_assert_eq!(retry.facts(), f);
    }

    #[test]
    fn cancellation_after_generated_history_stops_attempts_but_allows_valid_late_receipts(
        f in facts(), history in prop::collection::vec((path(), failure()), 0..25), hold in 0u8..3,
    ) {
        let mut delivery = Delivery::queued(f);
        for (p, reason) in history {
            delivery = delivery.reduce(DeliveryEvent::StartAttempt(p)).unwrap();
            delivery = delivery.reduce(DeliveryEvent::AttemptFailed { attempt: delivery.attempts(), reason }).unwrap();
        }
        delivery = match hold {
            1 => delivery.reduce(DeliveryEvent::IdentityChanged).unwrap(),
            2 => delivery.reduce(DeliveryEvent::AuthorizationDenied).unwrap(),
            _ => delivery,
        };
        let canceled = delivery.reduce(DeliveryEvent::Cancel).unwrap();
        prop_assert_eq!(canceled.state(), DeliveryState::Canceled);
        prop_assert_eq!(canceled.facts(), f);
        prop_assert_eq!(canceled.attempts(), delivery.attempts());
        prop_assert_eq!(canceled.path(), delivery.path());
        prop_assert_eq!(canceled.last_failure(), delivery.last_failure());
        for p in [DeliveryPath::Direct, DeliveryPath::Relay] {
            prop_assert_eq!(canceled.reduce(DeliveryEvent::StartAttempt(p)), Err(TransitionError::InvalidTransition));
        }
        prop_assert_eq!(canceled.reduce(DeliveryEvent::Cancel), Ok(canceled));
        let late = canceled.reduce(DeliveryEvent::Receipt(evidence(f, 0)));
        if canceled.attempts() == 0 {
            prop_assert_eq!(late, Err(TransitionError::UnsolicitedReceipt));
        } else {
            prop_assert_eq!(late.unwrap().state(), DeliveryState::DeviceDelivered);
        }
    }
}
