# Kami — Communication Architecture

**Status:** Owner-approved M1 architecture direction; implementation-specific interoperability must be validated  
**Language:** Rust  
**Coverage:** M1 component boundaries and network paths

## 1. Architectural invariants

1. Kami transports authenticated **logical messages** independently of whether packets use direct P2P or an authorized live relay.
2. Message ID, recipient, cryptographic identity, and delivery meaning remain stable across retries, path changes, and relay selection.
3. **Relay forwarding is not offline message storage.** M1 stores undelivered messages only on the sender; both parties must be online simultaneously to complete delivery.
4. A relay is not a trusted plaintext recipient. A separate verified end-to-end cryptographic session protects application messages regardless of transport.
5. Incoming communication never implies authority to run arbitrary commands or agent tools.

## 2. Components and ownership

```text
Human CLI                     Future harness adapters (not M1)
   |                                      |
   +---------- local authenticated IPC --+
                       |
                 Kami background service
       +---------------+----------------+
       |               |                |
  Identity / E2EE  Message engine   Connectivity manager
  verified device  Proto, receipts   rust-libp2p QUIC/TCP
  Double Ratchet   ordering, retry    NAT / hole punching
       |               |                |
       +------ encrypted SQLite --------+
               outbox / inbox
                       |
             Attempt authenticated direct P2P
                       |
             If unavailable: permitted live relay
                       |
             Recipient Kami background service
                  verified inbox + device ACK
```

- **CLI:** Sends commands, presents contacts/history/queue state, and displays errors; it neither stores identity secrets nor owns network sockets directly.
- **Local IPC boundary:** Unix domain socket (Linux/macOS) or named pipe (Windows), with OS access restrictions and application-level authorization as appropriate. Socket/pipe access must not automatically grant future agent permissions.
- **Background service:** Manually launched; owns network connections, crypto session state, receive processing, retry scheduling, relay reservations, and durable local state. No auto-start requirement.
- **Identity/E2EE manager:** Maintains independently verified contacts, local keys, binding from verified device identity to authenticated transport Peer ID, and session ratchet state.
- **Message engine:** Validates versioned Protobuf envelopes, assigns IDs and order, durably queues outbound messages, handles ACKs, deduplication, and history.
- **Connectivity manager:** Attempts authorized direct P2P; selects/maintains permitted relay connections; retries with backoff.
- **Independent relay processes:** Forward end-to-end-protected live streams subject to operator-managed identity and source/destination allowlists and resource controls. They do not retain durable offline message queues.

## 3. Participant, trust, and reachability

For M1, one verified participant identity maps to one active Kami device. Contact invitations carry public identity descriptors and network/relay hints. Their authenticity is established through independent fingerprint comparison and binding checks, **not** by trusting the relay or an invitation display name. Subsequent session establishment verifies the expected recipient, including authenticated libp2p Peer ID binding. An unexpected identity change stops messaging pending reverification.

Every endpoint may have a different list of candidate relays. Relay hints in a verified contact invitation may add candidates, but a usable route exists only if the recipient is actually reachable/reserved there and operator admission permits the relevant connection. **Relays do not need to exchange or forward traffic to each other.**

## 4. Network stack and path selection

- **Connectivity foundation:** rust-libp2p.
- **Direct transports:** QUIC preferred, TCP available when appropriate; authenticate connection identities. Use applicable, compatible NAT traversal/hole-punching mechanisms.
- **Path policy:** Bounded **direct-first** attempts. On failure, try authorized, reachable relay candidates in preference order; move to the next on failure.
- **Relay reachability:** While running, each endpoint establishes/renews eligible reservations with approved relays and recovers after reservation expiry or relay restart.
- **Relay admissions:** Operators maintain authenticated identity allowlists and explicitly permitted source/destination forwarding policies. Deny unidentified/unauthorized connections. Resource quotas and connection limits must apply.
- **Failure:** If no path works, preserve the sender's encrypted local queued message. Reconnection uses bounded exponential-style backoff and can resume after restarting the service.

**Implementation gate:** The Architect must check the current rust-libp2p QUIC/TCP transport, hole-punching, DCUtR, and relay capabilities in the selected versions; verify which combinations work in the intended operating systems. Do not imply all features are interchangeable or automatically supported by every transport.

## 5. Send, retry, receipt, and restart lifecycle

```text
CLI submits text
  -> validate destination, trust and 64-KiB encoded limit
  -> generate logical ID + sender sequence
  -> create E2EE-protected envelope
  -> durably commit encrypted sender outbox entry
  -> attempt direct P2P; else permitted prioritized relay
  -> recipient validates identity, authentication, ratchet and ID/content
  -> recipient atomically persists encrypted inbox/dedup state
  -> recipient emits authenticated DEVICE_DELIVERED ACK
  -> sender validates ACK and durably updates delivery state
```

If receipt is lost, the sender retransmits **the same logical message ID**; the recipient does not store/show a duplicate and may resend the ACK. Distinguish a retransmission of one immutable logical message from a new message; ratchet/session cryptography must support reliable retransmission and avoid reusing unsafe nonces or treating new ciphertext as a different logical message. A reused message ID carrying conflicting authenticated content must be rejected.

For a temporarily unavailable recipient, local retention lasts until delivery or explicit cancellation, within enforced resource limits. M1 includes **no mailbox upload/fetch**, no assumption that one device can send while the other is offline, and no false Delivered status for relay acceptance or socket transmission.

## 6. Ordering, history, deletion

Each sender maintains a monotonically ordered sequence within a conversation. The recipient durably stores valid later messages but briefly buffers their presentation if an earlier sequence is absent. After a bounded wait it may show a visible gap and later reconcile when the missing message arrives. There is no global total ordering across all users.

Encrypted SQLite stores sender outbox, recipient inbox, messages/history, receipts, deduplication facts, ordered state, and ratchet/session persistence as needed. SQLite transaction boundaries must ensure that a device-delivered ACK never precedes durable receipt. History persists across restart; local deletion and queued-message cancellation are separate, explicit operations. OS secure key storage protects key material; SQLite encryption does not itself solve backup/WAL/key exposure.

## 7. E2EE and security separation

- **Transport security:** Authenticate network peers across QUIC/TCP/direct/relay paths.
- **Application E2EE:** Authenticated Signal-style session establishment and Double Ratchet maintain recipient-only content confidentiality independent of the transport. Protocol and Rust library choices require interoperability/security validation before implementation.
- **Local protection:** Encrypt local message contents and protect keys through supported OS credential storage. No plaintext fallback if unavailable.
- **Threat boundaries:** A relay can observe some routing/IP/time/size metadata, deny service, and enforce policy, but should not be capable of decrypting application message bodies. Endpoint compromise remains outside an E2EE guarantee.
- **Input safety:** Validate unauthenticated/untrusted invitations, Protobuf envelopes, lengths, identities, receipts, and resource usage before consequential processing. Future agent tasks require separate execution authorization.

## 8. Scope separation

| M1 communication design | Future communication responsibilities |
|---|---|
| One-to-one verified device messaging | Multi-device accounts and recovery |
| Direct P2P and independently operated live relays | Federated deployment coordination |
| Sender-side durable retry outbox | Durable recipient offline mailboxes |
| E2EE text and encrypted local history | Groups, MLS/fanout, artifacts |
| Background service, CLI, private local IPC | Harness adapters and remote agent task semantics |

Shared hosting is allowed as an operator choice, but **multiple relays do not mean federation**, and an organization is not the same thing as a relay instance.

## 9. Implementation ownership and qualification

Engineering agents define specific async runtime wiring, relay process configuration, reconnection policy constants, Protobuf schema details, error codes, local IPC framing, tables, and tests, following approved invariants. They must not silently weaken E2EE, fail-open on identity checks, or drop an approved target OS. Validate integration across Linux/Windows/macOS and across real networks; explicitly force direct failure and verify genuine authorized relay fallback. Tests must cover restart, ACK loss, duplicate/conflict, ordering gaps, unavailable relay, changed identity, and key-store failure. Detailed encrypted message and identity contracts are owned by the corresponding canonical security/protocol specifications.
