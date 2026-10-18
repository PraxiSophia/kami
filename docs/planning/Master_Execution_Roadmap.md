# Kami M1 Implementation Roadmap

## Boundary

M1 is verified one-to-one private text between two human-operated CLI endpoints on Linux, Windows, and macOS. Each participant has one active device. The sender uses encrypted local persistence, authenticated direct P2P with supported NAT traversal, and authorized live-relay fallback through configured independent relays. The recipient persists before acknowledging delivery.

M1 excludes offline mailboxes, groups, artifacts, agent tasks, federation, multi-device recovery, anonymous routing, universal NAT reachability, and production SLAs.

## Workstreams

1. Workspace and dependency control
   - Establish the Rust workspace, binaries, lockfile, supported targets, and reproducible build checks.
2. Identity and E2EE
   - Implement OS-backed identity/key storage, invitations, fingerprint verification, Peer ID binding, Signal-style session establishment, Double Ratchet persistence, and fail-closed identity changes.
3. Protocol and local state
   - Implement versioned Protobuf frames, 65,536-byte encoded-message enforcement, stable IDs, content binding, ordering, receipts, encrypted SQLite state, outbox/inbox transactions, retry, deduplication, cancellation, and queue pressure.
4. Service and CLI
   - Implement the manually started background service, local authenticated IPC, commands, JSON/text output, diagnostics, profile ownership, and restart reconciliation.
5. Connectivity and relays
   - Implement QUIC-preferred/TCP-alternative rust-libp2p connectivity, supported NAT traversal, direct-first bounded selection, multiple configured relay candidates, relay admission, reservation renewal, and live fallback.
6. Qualification
   - Run component, cross-platform, direct-network, NAT/hole-punching, multi-relay, fallback, security, restart, and failure tests. Preserve evidence for the exact tested profiles.

## Dependencies

Identity and E2EE constrain protocol and transport. Protocol and local-state contracts constrain the service and CLI. The networking implementation depends on identity binding and message framing. Qualification depends on all workstreams and on named OS/network/relay profiles.

Engineering agents may select ordinary fields, crate versions, schemas, retry intervals, IPC framing, and diagnostics within the four canonical documents. Any change to M1 scope, E2EE, identity verification, OS coverage, relay privacy, or delivery semantics requires escalation.

## Completion claim

M1 is complete only when the canonical acceptance criteria pass with reproducible evidence on the supported operating environments. Passing a local test or a forced relay test alone is insufficient. Completion does not activate future mailbox, group, artifact, adapter, task, federation, or multi-device work.
