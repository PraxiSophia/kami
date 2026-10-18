# Kami Rust Implementation Decisions

## Purpose

Rust is Kami's implementation language. This document records engineering constraints for M1; exact dependency versions, APIs, platform support, and interoperability are selected and verified during implementation.

## M1 component direction

- A manually started Rust background service owns one profile, network connections, retries, receive processing, and local state.
- A Rust CLI communicates with the service through authenticated local IPC: Unix domain sockets on Linux/macOS and named pipes on Windows.
- rust-libp2p provides peer connectivity. QUIC is preferred and TCP is an alternative; supported NAT traversal or hole punching is used where validated.
- Independently operated relay processes provide live fallback and do not store offline messages.
- SQLite stores encrypted local outbox, inbox/history, receipts, deduplication, ordering, and ratchet/session state.
- OS-backed secure credential facilities protect identity and local encryption keys. Unavailable or locked key storage fails closed.

## Security boundary

Application E2EE uses the approved Signal-style session-establishment and Double Ratchet direction. Transport authentication and relay admission do not replace application E2EE or recipient identity verification. The selected Rust implementation must be maintained, reviewed, interoperable, cross-platform for the supported targets, and able to persist ratchet state safely.

The verified device identity must bind to the authenticated rust-libp2p Peer ID on direct and relayed sessions. Unknown or changed identities block sends and require explicit reverification. No custom cryptography or plaintext key fallback is permitted.

## Durable-state boundary

The sender commits the encrypted outbox record before network I/O. The recipient atomically commits inbox, deduplication, ordering, receipt, and display facts before emitting `device_delivered`. SQLite work and key-provider calls must not block the networking task. WAL, temporary files, backups, crash artifacts, and database encryption are separate local-protection concerns and require testing.

## Engineering-owned details

The implementation team selects exact crate versions, feature flags, Protobuf field numbers, IPC framing, schema migrations, retry intervals, resource limits, diagnostics, and process supervision within the canonical requirements. Each choice must be covered by tests and recorded in the implementation issues. Material changes to M1 scope, E2EE, identity verification, OS support, relay privacy, or delivery meaning require escalation.

## Feasibility and qualification

Before relying on a dependency or transport capability, verify the pinned build on Linux, Windows, and macOS as applicable. Test direct delivery, supported NAT traversal, multiple relay candidates, genuine automatic fallback, relay admission, key-provider failure, restart, lost ACK, duplicate delivery, ordering gaps, malformed input, and safe diagnostics. Results are limited to the tested configuration and do not constitute a security audit or universal network guarantee.
