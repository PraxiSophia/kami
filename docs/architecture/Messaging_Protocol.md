# Kami — Messaging Protocol and Durable Delivery

**Status:** Owner-approved M1 message semantics; concrete wire fields are engineering-owned  
**Scope:** Versioned Protobuf one-to-one private text, receipts, local persistence, retry, and ordering

## 1. Protocol invariants

- One user action creates **one logical message** with an immutable, stable `message_id`.
- Switching from direct P2P to a live relay, reconnecting, or retransmitting after a lost ACK does not create another logical message.
- The recipient must validate sender/recipient identity, cryptographic authenticity, content binding, version, size, and authorization before accepting a message.
- **Device-delivered** means the recipient has durably saved the authenticated message and the sender has validated its corresponding ACK; sending to a socket/relay is not delivery.
- Transport delivery attempts can be at-least-once; inbox persistence/display is deduplicated. This is **not** exactly-once physical transmission or future exactly-once task execution.
- The server-side offline mailbox is not part of M1. The sender's local durable queue holds messages until successful delivery or cancellation.

## 2. Wire framing and versioning

M1 encodes protocol frames using **Protocol Buffers**. A network message's total encoded size, envelope included, must not exceed **65,536 bytes**. The usable application text is therefore smaller than 64 KiB. Incoming frames must be bounded before allocation/decoding; malformed or unknown mandatory/unsupported versions fail safely.

Provide explicit version/feature negotiation and deterministic parsing rules. Choose field numbering/compatibility semantics that permit forward-compatible additions without silently reinterpreting unknown security-critical data. The engineer-owned `.proto` schema must specify at least:

| Concept | Required meaning |
|---|---|
| `protocol_version` | Version/compatibility boundary |
| `message_id` | Globally collision-resistant logical message identifier |
| `conversation_id` | The authenticated 1:1 conversation scope |
| `sender_device`, `recipient_device` | Verified cryptographic identity references, not display names |
| `sender_sequence` | Monotonic ordering per sender in the conversation |
| `message_kind` | M1 text or device-delivery receipt; future kinds are separately authorized |
| `created_at` | Sender-generated timestamp for display/diagnostics, not an authentication or total-order oracle |
| `protected_payload` | End-to-end authenticated and encrypted message content |
| `authentication/context` | Verifiable binding between payload, routing intent, participants and logical ID |

The actual schema may combine or encrypt fields differently to minimize metadata. The above table specifies **semantics**, not an exact plaintext wire layout. Routing intermediaries see only fields strictly needed to forward traffic.

## 3. Sender-side state and behavior

1. Validate caller permissions, verified contact identity, message type, text encoding, and encoded-size bound.
2. Allocate a stable message ID and sender-sequence number.
3. Generate protocol-protected content using the established E2EE session; bind immutable application content to logical identity and context.
4. **Transactionally persist** the encrypted message to the local SQLite outbox before attempting a network send.
5. Attempt bounded authenticated direct connectivity; if unsuccessful, try reachable/authorized relay candidates by priority.
6. On failed attempt, lost ACK, closed connection, or unavailable recipient, preserve the queue record and retry using bounded backoff after reconnect/restart.
7. On authentic ACK for the expected message and recipient, transactionally update the outbox to Delivered. Discarding/compacting committed delivery records must not undermine receipt conflict detection.
8. If explicitly canceled before delivery, stop future attempts; cancellation cannot recall a message already delivered.

The crypto/session implementation must safely reuse stored retransmission material or another reviewed scheme: a retry must not accidentally re-encrypt or ratchet in a way that creates a conflicting logical payload or repeats insecure nonces.

## 4. Recipient-side validation and durability

On receipt, Kami:

1. Checks frame length/version, authenticated remote device/Peer-ID binding, intended recipient and conversation, cryptographic integrity, and message-ID/content binding.
2. Atomically checks/inserts the deduplication key (authenticated sender identity + logical message ID), accepted immutable-content binding, sequence/state, and encrypted inbox record.
3. Rejects same-ID/different-content conflicts rather than accepting a revised message as a retry.
4. Only after the durable transaction commits, emits an authenticated `device_delivered` receipt.
5. If a previously accepted same-ID/same-content message returns, does not create a second inbox item or duplicate display; may resend the appropriate ACK.

A process crash before durable commit must not produce a valid Delivered receipt. A crash after commit but before sending ACK is recovered through same-ID sender retry and replay-safe deduplication.

## 5. Delivery states and acknowledgments

| State / event | Meaning |
|---|---|
| `queued` | Local sender transaction has durably retained the message |
| `in_flight` | A direct or relayed attempt is underway |
| `attempt_failed` | A particular path or attempt failed; queued message can still retry |
| `device_delivered` | Authenticated recipient durably saved it and sender verified recipient receipt |
| `blocked_identity` | Verified contact identity no longer matches; delivery is suspended |
| `blocked_authorization` | An authorization condition prevents current delivery |
| `canceled` | Sender explicitly terminated retry attempts |

Receipts must bind to authenticated sender/recipient, message ID, conversation, and appropriate integrity/authentication context. The sender must reject forged, stale, or mismatched receipts. Relay admission or raw socket write never promotes a message to `device_delivered`.

Read/display, user acknowledgment, future agent task acceptance and task completion are **separate semantics**; M1 does not make them implicit delivery receipts.

## 6. Ordering and missing sequence handling

Ordering is **per sender per conversation**, never a global total order across different people. The sender commits a monotonically increasing sequence with each new logical message. The recipient may durably accept authenticated sequence `N+2` before `N+1`, but:

- Temporarily buffer **presentation** of later messages for a bounded wait.
- If the earlier sequence is still missing, reveal the later message with an explicit gap indication.
- If the missing message later arrives, reconcile the displayed history deterministically without duplicating or discarding either item.
- Avoid indefinite head-of-line blocking; the concrete gap wait and bounds are engineering-owned and covered by tests.

Sequence values do not replace message IDs, cryptographic anti-replay checks, or per-contact authorization.

## 7. Retry and resource discipline

Messages stay in the sender's local encrypted outbox **until delivered or manually canceled**, not until an arbitrary time-to-live. Use bounded retry rates (backoff, jitter, per-peer and relay throttles), connection/resource limits, maximum queued message count/bytes, and explicit queue-pressure errors. When capacity is reached, do not silently drop accepted queued messages; instead reject new enqueue operations or request explicit user action per a defined safe policy.

On service restart, reconstruct pending queues and ratchet/message state transactionally, resume only authorized sends, and validate current contact keys before retry. When a contact's identity unexpectedly changes, hold—not redirect—queued messages pending reverification.

## 8. Local persistence and history

Use SQLite for durable per-endpoint data: verified contacts/reference identities, immutable logical message records, sender sequence allocation, outbox state, inbox/deduplication records, authenticated receipts, presentation ordering/gaps, persistent encrypted history, and supported crypto session state.

- Sensitive message content is encrypted locally; key custody uses approved OS secure-storage integration.
- Crash consistency must cover durable enqueue-before-send, inbox-before-ACK, ratchet-state consistency, and duplicate detection after restart.
- A locally deleted message/history item must not silently cancel still-pending transmission. Conversely, explicit cancel must stop outbox retries without claiming remote deletion.
- Retain compact, protected deduplication evidence as needed after history deletion so delayed retries do not reappear as newly delivered messages.
- Handle WAL/temporary data, backups, and logs in the local security design.

## 9. Error cases and test matrix

| Case | Required result |
|---|---|
| Direct network blocked | Attempt compatible authorized relay; same message ID |
| All relays unavailable | Message remains queued locally; retry later |
| Recipient offline | No server mailbox; sender queues until both are online |
| Sender restarts with pending outbox | Safely resume scheduled attempts with same logical ID |
| Recipient crashes before persistence | No durable delivery ACK; sender retries |
| Recipient crashes after persistence before ACK | Retry suppressed as duplicate; ACK can be resent |
| Duplicate received by direct and relay paths | One durable logical inbox item and visible message |
| Same ID but conflicting authenticated content | Reject and surface security/protocol error |
| Forged / wrong-device receipt | Reject; no false Delivered state |
| Message sequences 1, 3, then 2 | Bounded wait/gap; deterministic reconciliation |
| Oversized / malformed Protobuf | Reject without excessive allocation or crash |
| Changed identity key | Suspend pending delivery until explicit reverification |
| Service restart during crypto exchange | No unsafe nonce reuse or broken ratchet persistence |
| Cancel queued message | Stop retries, with no promise of recalling already delivered data |

## 10. Engineering contracts to finalize before code integration

Engineering agents own the exact Protobuf field numbers/encoding, canonical signature/authentication context, ID generation, clock handling, SQLite DDL/migrations, receipt signature/MAC details, transaction and crash-recovery design, ordering timeout, retry schedules, and resource bounds. Each choice requires tests and must remain consistent with the canonical identity and architecture contracts. Material violations of approved confidentiality, trust, portability, or M1 scope require owner escalation, not silent substitution.
