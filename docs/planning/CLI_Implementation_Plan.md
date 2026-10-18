# Kami CLI and Background Service

## M1 boundary

The CLI is a client of a manually started background Kami service. M1 supports identity initialization, invitation import/export, fingerprint verification, service control, one-to-one text send/receive, history, queue status, cancellation, and diagnostics. It does not silently install or enable automatic startup.

## Local IPC

The CLI uses authenticated local IPC:

- Unix domain sockets on Linux and macOS.
- Named pipes on Windows.

There is no public local TCP API by default. IPC access must be restricted by the operating system and application authorization. Future adapters use this boundary rather than accessing SQLite or key material directly.

## Service responsibilities

The service owns identity/session state, encrypted SQLite access, network connections, direct and relay path selection, relay reservations, receive processing, retry scheduling, ordering, receipts, and shutdown/restart reconciliation. One active service owns a profile at a time. SQLite and secure-key operations run outside the continuously polled networking task.

## Commands and output

The exact command names and IPC schema are implementation-owned, but the interface must provide equivalent operations for:

- Initialize a profile and display its public descriptor.
- Import and export a contact invitation.
- Verify or block a contact fingerprint.
- Start, stop, and inspect the background service.
- Send text to a verified contact.
- List and display local history.
- List queued, in-flight, blocked, failed, canceled, and delivered messages.
- Cancel an undelivered queued message.
- Receive messages and inspect safe diagnostics.

Human and JSON output use the canonical states `queued`, `in_flight`, `delivered`, `blocked`, `failed`, and `canceled`. A socket write, relay acceptance, or connection establishment never produces `delivered`. `read` is not an M1 receipt.

Output must escape control characters and treat message text as untrusted data. Default logs and diagnostics exclude message bodies, private keys, secrets, and raw private endpoint data. Errors identify a safe reason class and whether retry is possible.

## Lifecycle and safety

The service commits the encrypted outbox before sending and resumes authorized pending work after restart. It does not silently replace changed identities, regenerate lost identities, downgrade unavailable key storage to plaintext, or grant future adapters database/key access. Local history deletion and queued-message cancellation are separate operations; neither promises deletion of a recipient's copy.

## Verification

Black-box tests must cover IPC authorization, profile locking, safe rendering, JSON and exit-status behavior, service interruption, restart reconciliation, queue pressure, key-store unavailability, duplicate delivery, ordering gaps, changed identities, relay failure, and truthful delivery states on Linux, Windows, and macOS.
