# Kami P2P Connectivity

## Scope

This document defines M1 connectivity behavior. Kami uses Rust and rust-libp2p. M1 supports authenticated direct peer communication, supported NAT traversal and hole punching, and fallback through multiple independently operated live relays. It does not provide an offline mailbox or inter-relay federation.

## Identity and reachability

A contact is addressed by its verified participant/device identity, not by a display name, IP address, or mutable network address. Contact invitations may contain public identity material, Peer ID binding evidence, candidate addresses, and relay hints. Invitations and reachability hints are untrusted until the identity fingerprint is independently verified.

Every direct or relayed session must authenticate the expected device and verify its binding to the authenticated libp2p Peer ID. Relay authentication alone does not authenticate the recipient. Unknown, changed, or mismatched identity bindings fail closed.

Candidate addresses and relay hints are bounded and treated as untrusted input. They must not be derived from message content or permit arbitrary dialing. Presence and relay reservation are reachability hints, not delivery evidence.

## Transport and path selection

- QUIC is preferred when the selected rust-libp2p configuration and target platform support it.
- TCP is an alternative transport when QUIC is unavailable or unsuitable.
- The client attempts an authenticated direct path first within a bounded policy.
- Supported NAT traversal or hole-punching mechanisms may be used when enabled by the selected transport profile.
- If direct delivery cannot complete, the client tries configured authorized relays in priority order and fails over between candidates.
- Direct and relay attempts reuse the same logical message ID and content binding.
- A security failure is not a reason to downgrade authentication or encryption.
- If all paths fail, the encrypted message remains in the sender's local outbox until delivery or explicit cancellation.

The service must be manually started and maintain eligible relay reservations while running. M1 requires sender and recipient to be online concurrently; a relay does not store messages after the live path ends.

## Relay boundary

Each relay is an independently operated process with its own identity, admission policy, resource limits, health state, and operator configuration. Relay admission uses authenticated allowlists and source/destination policy. Unauthorized reservations and forwarding attempts must be denied before useful forwarding.

A relay forwards live end-to-end-protected traffic. It may observe routing metadata such as addresses, timing, sizes, and transport identities, but must not require application plaintext or private keys. Relay operation is not recipient authentication, offline storage, anonymity, or an availability guarantee.

## Client ownership and lifecycle

One background service owns each profile's connections, path selection, retry scheduling, receive processing, and local state. A continuously polled networking task must not be blocked by SQLite, key-provider calls, CLI input, or application work. Bounded command/event channels connect networking and delivery components.

The delivery engine records the actual connection and path used for each frame and receipt. A successful socket write, relay reservation, or relay forwarding event cannot produce `device_delivered`; that state requires the recipient's durable commit and a validated receipt.

## Verification requirements

The implementation must qualify:

- Direct delivery on a supported cross-network profile.
- Supported NAT traversal or hole punching where claimed.
- Automatic fallback after a genuine direct-path failure.
- Multiple configured relay candidates and priority/failover behavior.
- Unauthorized relay reservation and source/destination denial.
- Relay restart, reservation renewal, quotas, and bounded resource exhaustion.
- Wrong or changed identity, malformed input, lost ACK, duplicate cross-path delivery, and no-route behavior.

Results apply only to the recorded OS, transport configuration, network profiles, and relay versions tested. They do not establish universal NAT reachability, anonymity, or an SLA.
