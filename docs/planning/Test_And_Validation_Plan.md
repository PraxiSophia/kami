# Kami M1 Test and Validation Plan

## Purpose

This plan qualifies the approved Milestone 1 behavior. It separates component evidence from cross-network qualification and does not turn tests for future mailboxes, groups, artifacts, or adapters into M1 results.

## Required test profile

Every run records the exact application build, Rust/dependency configuration, operating system, relay configuration, fixture identifiers, topology, fault schedule, safe participant/device identifiers, logical message IDs, paths, state transitions, and pass/fail disposition.

M1 qualification covers Linux, Windows, and macOS, subject to supported key-storage and dependency validation. Network evidence uses two endpoint networks and independently operated configured relay candidates. A same-host simulation cannot establish direct cross-network or automatic-fallback claims.

## Component and local integration tests

- Identity initialization creates an immutable device identity and uses OS-backed secure storage. Locked or unavailable storage fails closed.
- Invitation import remains unverified until independent fingerprint verification. Unknown, changed, wrong-device, and wrong-Peer-ID cases are rejected.
- Signal-style session establishment and Double Ratchet state persist safely across restart, retransmission, and bounded out-of-order delivery using the selected reviewed Rust implementation.
- Protobuf frames reject malformed, unknown-required-version, oversized, invalid-encoding, and unauthorized input before unbounded allocation.
- Sender outbox persistence precedes network I/O.
- Recipient inbox, deduplication, history, ordering, and receipt facts commit atomically before `device_delivered`.
- Same-ID/same-content duplicates re-ack without a second display; same-ID/different-content conflicts are rejected.
- Lost ACK, restart, cancellation, storage-full, key-store failure, profile lock, and queue-pressure behavior remain explicit and safe.
- CLI IPC authorization, output escaping, JSON/text status, exit codes, and service lifecycle behave consistently on all supported operating systems.

## Network and relay qualification

1. On a recorded direct-eligible cross-network profile, deliver a message over the actual direct path and validate the recipient receipt.
2. On a separate controlled profile, make direct delivery unavailable while preserving access to configured relays. Use normal automatic path selection, not a forced relay flag.
3. Verify authorized relay candidates are tried in priority order and fail over when a relay is unavailable.
4. Verify relay identity, reservation admission, source/destination policy, quotas, connection limits, rate limits, duration limits, and restart/renewal behavior.
5. Confirm the relay cannot decrypt application content and that diagnostics do not expose secrets or plaintext.
6. Verify direct/relay duplicates, path changes, interrupted links, ACK loss, no route, and changed identities preserve one logical message and safe state.

## Acceptance dispositions

Each case is `pass`, `fail`, `invalid`, `excluded`, or `not testable`. A case is invalid if the declared fault or actual path was not demonstrated. A case is not testable when the selected implementation, supported platform, limit, or environment is unavailable. No result claims universal NAT traversal, anonymity, a production SLA, or a cryptographic audit.

## Deferred test suites

Offline mailbox delivery, asynchronous E2EE, group membership and encryption, artifacts, multi-device recovery, agent adapters, task authorization, federation, and full-v0.1 operations require separate contracts and qualification. Their tests must not be reported as M1 evidence.
