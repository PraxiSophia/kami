# kami-transport

Authenticated, bounded direct-connection substrate for **KAMI-27** in the Kami architecture.

This crate provides direct peer-to-peer transport over libp2p (QUIC with TCP/Noise/Yamux fallback), validating authenticated remote Peer IDs against verified device identities and issuing revocable capabilities for runtime supervision.

---

## 1. Overview & Purpose

In Kami's communication-first architecture, transport is strictly an untrusted byte and connection pipe. It does not own cryptographic keys, persistent message storage, or end-to-end encryption.

`kami-transport` implements the **KAMI-27** bounded direct-transport milestone:
- **Device Identity & Custody**: Uses sealed credentials (`SealedTransportCredentials`) derived from `kami-identity` key custody.
- **Mutual Cryptographic Authentication**: Connections are mutually authenticated via libp2p TLS 1.3 (for QUIC) or Noise XX (for TCP/Yamux).
- **Enforced Trust Boundary**: A remote `PeerId` is accepted **only** through the actual authenticated cryptographic handshake verified against fresh, bilateral `ContactTrust` records. A connection is never authorized merely because an external caller or message envelope asserts a matching string.
- **Revocable Capability Model**: Successful connections yield an unforgeable, linear `AuthenticatedConnection` capability bound to an active profile lease, a specific trust generation, and an expiration TTL.
- **Strictly Decoupled**: Does not implement application streams, end-to-end encryption, framing, message delivery, ACKs, or receipts.

```
+----------------------------------------------------------------------------+
|                             kami-runtime                                   |
|   (Supervises Driver in Tokio JoinSet, holds ProfileLease, controls dials)  |
+-------------------------------------+--------------------------------------+
                                      | Driver::new(credentials, config)
                                      v
+----------------------------------------------------------------------------+
|                            kami-transport                                  |
|                                                                            |
|   [Policy]                 [Driver / Swarm]           [Connection]         |
|   - Multiaddr pre-filter   - libp2p Swarm (QUIC/TCP)  - Authenticated-     |
|   - Max 8 candidates       - Bounded dial queue         Connection         |
|   - Timeout/TTL bounds     - Expected-peer binding    - Revocation flag    |
+-------------------------------------+--------------------------------------+
                                      | SealedTransportCredentials
                                      v
+----------------------------------------------------------------------------+
|                            kami-identity                                   |
|   (OS Keyring custody, Peer-ID binding, ContactTrust state verification)   |
+----------------------------------------------------------------------------+
```

---

## 2. Key Architecture & Trust Boundaries

### 2.1 The Direct Connection Trust Boundary
A central architectural invariant of KAMI-27 is that **network transport cannot establish identity trust on its own**:
1. **Expected-Peer Dial Binding**: Outbound dials require both a device key (`[u8; 32]`) and verified multiaddresses. The transport driver dials libp2p with `DialOpts::peer_id(...)`, ensuring that if a malicious host responds at that socket address with a different Peer ID, libp2p aborts the handshake before connection establishment.
2. **Fresh Bilateral Verification**: Upon `ConnectionEstablished`, the driver checks the verified `ContactTrust` record in memory. If the peer is not in `ContactTrustState::Verified` or if `send_decision(...) != SendDecision::Allow`, the connection is immediately aborted.
3. **No Caller-Asserted Bypasses**: The capability's `authenticated_peer(&self, fresh: &ContactTrust)` method requires passing a fresh durable trust readback and validates that `fresh.generation() == self.generation`.

### 2.2 Missing Integration Boundary (Explicitly Deferred)
> [!IMPORTANT]
> `kami-transport` provides **transport connection capabilities only**. It is **not** connected to production E2EE or message delivery:
> - The connection capability is not yet bridged into `kami-sender`, `kami-receiver`, or `kami-e2ee`.
> - Production ingress via `Engine::enqueue` in `kami-runtime` continues to fail closed with `Error::LiveTransportUnavailable`.
> - Wire transmission of encrypted application payloads and receipt round-trips remains deferred to **KAMI-28**.

---

## 3. Source Code Explanation

### 3.1 Module `policy` ([crates/kami-transport/src/policy.rs](file:///home/phu/dev/kami/crates/kami-transport/src/policy.rs))

Enforces strict resource bounds and address validation before dialing or listening.

- **Struct**: `Config`
  - `max_connections: u32` (default: 16, allowed range: `1..=32`): Maximum concurrent established peer connections.
  - `max_dials: usize` (default: 4, allowed range: `1..=8`): Maximum concurrent active dial attempts.
  - `max_attempts: usize` (default: 8, allowed range: `1..=8`): Maximum candidate socket addresses per peer dial.
  - `dial_timeout: Duration` (default: 3s, ceiling: 10s): Strict per-attempt connect timeout.
  - `capability_ttl: Duration` (default: 30s, ceiling: 60s): Lifespan before an `AuthenticatedConnection` expires.
  - Method `validate(&self) -> Result<(), Error>`: Fails closed with `Error::InvalidConfig` if any bound is violated.

- **Function**: `direct_address(addr: &Multiaddr, listen: bool) -> Result<bool, Error>`
  - Restricts addresses to **exact IP socket candidates**:
    - IPv4/IPv6 + TCP (e.g., `/ip4/127.0.0.1/tcp/1234`).
    - IPv4/IPv6 + UDP + QUIC-v1 (e.g., `/ip4/127.0.0.1/udp/1234/quic-v1`).
  - **Prohibited protocols**: Rejects DNS names (`/dns/...`), WebSockets (`/ws`), web transports, circuit relays (`/p2p-circuit`), multicast addresses, and caller-asserted peer IDs (`/p2p/...`).
  - Limits total multiaddress length to 64 bytes.

---

### 3.2 Module `connection` ([crates/kami-transport/src/connection.rs](file:///home/phu/dev/kami/crates/kami-transport/src/connection.rs))

Defines the capability types issued upon successful direct connection.

- **Struct**: `Revocation`
  - Atomic boolean (`Arc<AtomicBool>`) providing monotonic invalidation.
  - Used by `kami-runtime` to instantly revoke all outstanding connection capabilities during shutdown, profile lease loss, or contact trust invalidation.
  - Calling `revoke()` permanently marks the capability invalid; it cannot be revived.

- **Struct**: `AuthenticatedConnection`
  - **Linear Capability**: Deliberately does **not** implement `Clone` or `Default`. It can only be minted internally by `Driver` upon an authenticated swarm event.
  - Fields:
    - `authority: Revocation`: Handle to runtime revocation state.
    - `peer: PeerId`: Cryptographically proven remote libp2p Peer ID.
    - `device: [u8; 32]`: Associated verified 32-byte device identity.
    - `generation: u64`: Identity trust generation at the time of connection.
    - `live: Arc<AtomicBool>`: Tracks whether the underlying libp2p swarm connection is active.
    - `expires: Instant`: Monotonic deadline based on `capability_ttl`.
  - Methods:
    - `is_current(&self) -> bool`: Checks `authority.current() && live && now < expires`.
    - `authenticated_peer(&self, fresh: &ContactTrust) -> Result<PeerId, Error>`: Verifies currentness, matches trust generation, executes `send_decision(...) == SendDecision::Allow`, and returns the verified `PeerId`.

- **Enum**: `Error`
  - Redacted, safe error categories: `InvalidConfig`, `InvalidAddress`, `Busy`, `Closed`, `Untrusted`, `Unavailable`, `Timeout`, `Cancelled`, `Expired`.

---

### 3.3 Module `driver` ([crates/kami-transport/src/driver.rs](file:///home/phu/dev/kami/crates/kami-transport/src/driver.rs))

Runs the polled libp2p `Swarm` actor inside the Tokio runtime.

- **Struct**: `Handle`
  - Cloneable, bounded control handle used by `kami-runtime`:
    - `dial(device, addresses) -> Result<AuthenticatedConnection, Error>`: Initiates bounded, cancellable outbound dialing to candidate multiaddresses.
    - `disconnect(device) -> Result<(), Error>`: Drops an active peer connection.
    - `register(contact) -> Result<(), Error>`: Injects freshly loaded durable contact trust into the transport driver.
    - `refresh_contact(device, contact) -> Result<(), Error>`: Bounded control update for identity changes.
    - `local_addresses() -> Result<Vec<Multiaddr>, Error>`: Returns active listening multiaddresses.
    - `revoke_all() -> Result<(), Error>`: Triggers immediate revocation of all capabilities.

- **Struct**: `Driver`
  - Owns the libp2p `Swarm<Behaviour>` configured with QUIC and TCP transports.
  - Manages dial queues, candidate address fallback, connection limit enforcement (`max_established_per_peer = 1`), and connection lifetime bookkeeping.
  - Polled sequentially inside a `tokio::task::JoinSet` child task.

- **Enum**: `Event`
  - Telemetry notifications emitted across an mpsc channel:
    - `Listening(Multiaddr)`: Listener bound to an interface and port.
    - `Connected([u8; 32])`: Direct connection established and authenticated.
    - `Disconnected([u8; 32])`: Connection dropped or timed out.
    - `AttemptFailed(Error)`: Dial attempt failure.
    - `Stopped`: Driver loop completed cleanly.

---

### 6. Module `prekey_exchange` ([crates/kami-transport/src/prekey_exchange.rs](file:///home/phu/dev/kami/crates/kami-transport/src/prekey_exchange.rs))

Introduced in **KAMI-51 WP-B** to provide a dedicated, bounded request-response wire subprotocol for prekey exchange:

- **Protocol ID**: `/kami/prekey-exchange/1` over libp2p `request_response::Behaviour`.
- **Framing & Size Limits**:
  - 4-byte big-endian length prefix.
  - Maximum body size is **16,384 bytes (16 KiB)** (`MAX_BODY_BYTES`).
  - Request and response substreams are half-closed immediately after the single fixed frame.
- **Concurrency & Timeouts**:
  - Request timeout: **5 seconds** (`Duration::from_secs(5)`).
  - Stream limit: At most **1 concurrent stream per peer connection** (`with_max_concurrent_streams(1)`).
  - Global limit: Global concurrency cap of **16 concurrent inbound operations** via atomic permit in `Codec`.
- **Message Types**:
  - Requests: `Exchange::Query` or `Exchange::Ack`.
  - Responses: `Exchange::Selection`, `Exchange::AckResult`, or `Exchange::Error` (with closed failure code 1..=11, zero free-text strings).

---

## 4. Operational & Manual Qualification Evidence

### KAMI-27 Direct Transport Qualification
KAMI-27 was accepted following automated tests and an owner-operated two-process manual qualification harness on Linux:
1. **Owner-Operated Qualification Harness**:
   - Helper scripts located at [`scripts/kami27-manual/`](file:///home/phu/dev/kami/scripts/kami27-manual/): `setup.sh`, `start-a.sh`, `start-b.sh`, `reject-negative.sh`.
   - Confirmed manual fingerprint comparison, explicit verification, QUIC and TCP authentication, disconnect/reconnect, and clean process termination on Linux localhost.
2. **Automated Test Coverage**:
   - [`crates/kami-transport/tests/direct.rs`](file:///home/phu/dev/kami/crates/kami-transport/tests/direct.rs): Multi-identity automated test verifying expected-peer dialing, address filtering, candidate fallback, and strict socket-negative rejection when an unexpected peer answers.

### KAMI-51 WP-B Prekey Endpoint Qualification
KAMI-51 WP-B extended the native manual endpoint harness for prekey exchange:
1. **Harness & Scripts**:
   - Helper scripts located at [`scripts/kami51-wpb-manual/`](file:///home/phu/dev/kami/scripts/kami51-wpb-manual/):
     - `verify.sh --check`: Verifies compilation, tool dependencies (`dbus-run-session`, `gnome-keyring-daemon`), and runtime tests.
     - `verify.sh --endpoint`: Launches `transport_tests::native::manual_endpoint` under private D-Bus and disposable home/keyring.
     - `verify.sh --fixtures`: Executes joint runtime, storage, and E2EE prekey tests in private child processes.
2. **Provenance Distinction**:
   - **Owner-Observed QUIC Exchange (Terminal Evidence)**: Verified live two-terminal QUIC run with terminal output: `PREKEY_LAST_ATTEMPT=completed`, `QUERY=durable`, `SIGNED_KIND=OneTime`, `FRESHNESS=valid-at-retention`, `RETENTION_BEFORE_ACK=committed`, `PUBLICATION_ACK_RESULT=exact-result-reconciled`. An initial `Transport(Expired)` failure and subsequent successful reconnect were observed and retained as valuable qualification evidence.
   - **Owner-Reported Scenarios (Pending Log Archival)**: Subsequent TCP, reverse, and retry command sequences reported successful by owner; awaiting local test log archival.
   - **Antigravity Execution Boundary**: Antigravity did **not** execute, simulate, or fabricate interactive manual tests.

---

## 5. Scope Boundary & Deferred Capabilities

`kami-transport` fulfills the bounded direct-transport milestone (KAMI-27). The following capabilities are explicitly deferred:

| Deferred Capability | Tracking | Architectural Status |
| :--- | :--- | :--- |
| **Application Message Payloads** | KAMI-28 | Wire transmission of encrypted application messages, receipts, and history streams. |
| **Production E2EE Ingress** | KAMI-28 / WP-D | Bridging live transport peer provenance into production `QuarantinedSession` decryption. |
| **Relay Service & Reservations** | KAMI-29 / KAMI-30 | Default-deny live relay protocol, circuit reservations, and relay packet forwarding. |
| **Automatic Path Fallback** | KAMI-31 | Direct-first racing, transparent relay failover, and preserving message identity across path switches. |
| **Local CLI / IPC Control** | KAMI-32 | Unix domain socket / named pipe daemon management and status rendering. |
| **NAT Traversal & Hole Punching** | Post-M1 | DCUtR, STUN, and ICE hole punching across restrictive symmetric NATs. |
| **Cross-Platform Qualification** | KAMI-34 / KAMI-35 | Native verification on Windows Credential Manager and macOS Keychain; real separate-network qualification. |

---

## 6. Implementation & Verification Commands

```bash
# Run unit and contract tests in kami-transport
cargo test --locked -p kami-transport

# Run the automated native QUIC, TCP, and fallback integration suite
cargo test --locked -p kami-transport --test direct real_native_quic_tcp_and_fallback -- --exact --nocapture

# Run compile-fail doctests (verifying capability cannot be default-constructed or cloned)
cargo test --locked -p kami-transport --doc -- --nocapture

# Run KAMI-51 WP-B manual verification check
bash scripts/kami51-wpb-manual/verify.sh --check

# Run KAMI-51 WP-B prekey test fixtures
bash scripts/kami51-wpb-manual/verify.sh --fixtures

# Verify formatting and warnings-denied Clippy checks across workspace
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
```

---

## 7. Interactive Architecture Visualizations

- **KAMI-VIS-002**: [Rust Crate Dependency Graph](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/01-System-Architecture/KAMI-VIS-002/crate-dependencies.html) — Documents `kami-transport` as the 11th workspace crate, connecting `kami-runtime` and `kami-identity`.
- **KAMI-VIS-027**: [Supervised Tokio Runtime & Task Ownership](file:///home/phu/dev/kami/knowledge/Code%20Analyzing/Visualizations/01-System-Architecture/KAMI-VIS-027/runtime-supervision.html) — Illustrates `TransportDriver` supervised within Tokio's `JoinSet`, issuing revocable `AuthenticatedConnection` capabilities while maintaining the fail-closed E2EE boundary.

