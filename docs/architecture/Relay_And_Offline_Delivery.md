# Kami Relay and Offline Delivery

## M1 live relay

M1 uses independently operated live relays only. A relay forwards authenticated end-to-end-protected traffic when a direct path is unavailable. It does not accept messages for later delivery and must never emit `mailbox_accepted` or `device_delivered` merely because it received or forwarded bytes.

Each relay has a separate identity, listener, reservation state, admission policy, quotas, and health reporting. Clients authenticate the relay separately from the recipient. The application session must authenticate the intended recipient device through its verified Peer ID binding. Relay operators may see routing metadata but must not need application plaintext or device private keys.

Clients maintain configured relay candidates and try them in priority order. Reservation renewal and relay interruption are handled while the background service is running. If the recipient is offline, relay access is denied, or all paths fail, the sender retains the encrypted message in its local outbox and reports a retryable or terminal state according to the protocol contract.

## M1 admission and resource controls

Relay policy must deny unauthorized reservations and unauthorized source-to-destination forwarding before useful forwarding. It must bound reservations, concurrent circuits, connections, bytes, rates, duration, and renewal. Limits account for transport, framing, encryption, multiplexing, and receipt overhead. A broad peer allowlist or post-accept event is not sufficient evidence of per-connection authorization.

Relay logs, metrics, and live buffers must exclude application plaintext, private keys, and unredacted sensitive diagnostics. Restart, renewal failure, quota refusal, and mid-stream exhaustion produce explicit client-visible outcomes.

## Future durable mailbox

An offline mailbox is a separate future service and protocol. It requires asynchronous application E2EE, sender authentication, recipient and organization authorization at enqueue and fetch, expiry, quotas, replay protection, poison-message handling, backup/recovery, and independently verifiable service receipts.

Captured live-session ciphertext cannot be stored for later recipient decryption. A future mailbox must define its own per-recipient ciphertext envelope, key lifecycle, receipt semantics, and migration/version boundary. `mailbox_accepted` means durable queue storage only; it does not mean device delivery, human read, or task acceptance.

On reconnect, the recipient authenticates, fetches authorized ciphertext, validates and decrypts it, commits local inbox and deduplication state, and only then emits a device-delivery receipt. Revocation and membership changes require an explicit cutoff policy; previously downloaded plaintext cannot be remotely erased.

## Verification boundary

M1 qualification must inspect live relay endpoint termination, admission, limits, restart behavior, metadata handling, and genuine automatic fallback. It must not claim offline delivery, mailbox durability, asynchronous E2EE, universal reachability, or production capacity. Future mailbox work requires its own approved contract and test matrix.
