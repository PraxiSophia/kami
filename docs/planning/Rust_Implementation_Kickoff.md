# Kami Implementation Kickoff

## Starting point

The four canonical documents define the approved M1 requirements, communication architecture, identity/E2EE contract, and messaging protocol. This document translates them into implementation order without adding requirements.

## Initial workspace

Create a Rust workspace with focused boundaries for:

- CLI and local IPC.
- Core message IDs, states, errors, and policy.
- Protobuf protocol and validators.
- Identity, OS key storage, contact verification, and E2EE session state.
- Encrypted SQLite persistence.
- Delivery engine and retry/reconciliation.
- rust-libp2p connectivity and relay clients.
- Independent relay service.

The workspace must build reproducibly on the supported targets. It must not expose private keys, open a public local API, or claim network capability before the relevant tests exist.

## Implementation order

1. Pin and verify Rust, dependency, crypto-library, OS key-storage, Protobuf, SQLite, and rust-libp2p compatibility for Linux, Windows, and macOS.
2. Implement profile initialization, protected identity storage, invitation import/export, fingerprint verification, Peer ID binding, and changed-key blocking.
3. Implement Signal-style session establishment and Double Ratchet persistence using the selected maintained Rust implementation. Escalate if no defensible implementation supports the required platforms and persistence semantics.
4. Implement bounded Protobuf parsing, stable logical IDs, content binding, per-sender ordering, receipt validation, and delivery-state transitions.
5. Implement encrypted SQLite migrations and transaction boundaries: outbox before send and inbox/dedup/receipt before ACK.
6. Implement the manually started service, authenticated local IPC, CLI status/output, restart reconciliation, cancellation, and safe diagnostics.
7. Implement QUIC-preferred/TCP-alternative direct connectivity, supported NAT traversal, relay candidate priority/failover, relay admission, and live fallback.
8. Run the M1 test and qualification plan on named operating systems, networks, and independent relays.

## Stop conditions

Stop and escalate rather than silently weakening the design if:

- The selected E2EE implementation lacks maintained, interoperable, cross-platform support.
- Key storage requires plaintext fallback.
- Device identity cannot bind to the authenticated Peer ID on direct and relayed paths.
- Protobuf bounds, encrypted local state, or crash recovery cannot be enforced.
- Relay admission cannot deny unauthorized forwarding before useful forwarding.
- A supported OS or network profile cannot meet the approved M1 behavior.
- A path or test would claim offline delivery, universal NAT reachability, anonymity, or an SLA.

## Evidence

Record exact toolchain and dependency versions, OS and network profiles, relay configurations, test fixtures, actual paths, state transitions, and redacted diagnostic results. Tests must cover identity changes, malformed input, duplicate/path races, ACK loss, restart, queue pressure, relay failure, ordering gaps, and unavailable key storage.
