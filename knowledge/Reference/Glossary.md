---
title: Kami Architecture Glossary
type: reference
status: current
---

# Architecture Glossary

- **ACK / receipt:** Confirmation. `device_delivered` means the recipient durably committed the authenticated message and the sender validated the receipt; a socket write is not delivery.
- **Double Ratchet:** Message-key progression used by the approved Signal-style application E2EE direction.
- **E2EE:** End-to-end encryption. Intended endpoints decrypt application content; relays forward protected traffic and may observe routing metadata.
- **FM / M1:** Milestone 1, the first bounded online communication scope.
- **IPC:** Local authenticated communication between the CLI and manually started background service: Unix domain sockets on Linux/macOS or named pipes on Windows.
- **Peer ID:** Authenticated libp2p transport identity bound to the verified Kami device identity.
- **Protobuf:** The M1 versioned wire encoding. Total encoded network message size is capped at 65,536 bytes.
- **P2P:** Direct endpoint communication rather than live relay forwarding.
- **Relay:** Independently operated live-forwarding process. It is not an offline mailbox and does not authenticate the recipient by itself.
- **Signal-style session:** The approved application E2EE direction; exact maintained Rust implementation and compatibility require verification.
- **SQLite:** Encrypted local durable state for outbox, inbox/history, receipts, deduplication, ordering, and session state.
- **TOFU:** Trust on first use. M1 does not permit private sends before explicit fingerprint verification.

Status words: **approved** means recorded in the canonical baseline; **unverified** means adequate implementation evidence does not yet exist; **complete** means the authoritative source records completion.

Sources: the four canonical documents and [[Architecture Sources]].
