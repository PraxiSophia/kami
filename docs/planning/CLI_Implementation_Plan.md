# CLI Implementation Plan

Status: Draft — Pending Approval

Revision task: `t_295267ab`, superseding the implementation proposals of `t_6aa15d26`. Rust is the owner's selected sole implementation language. All other technical selections, APIs, limits and tests below remain proposals; this document authorizes no implementation. No code, dependency installation, experiment, deployment, build or performance test was performed for this revision.

## 1. Authority, evidence, and milestone boundary

Source labels used throughout:

- **R** — `../Kami_Project_Requirements.md`, v0.1 requirements draft, itself proposed rather than approved.
- **D** — `../Kami_Communication_Design.md`, v0.1 architecture proposal, not a binding stack selection.
- **G** — `/home/phu/.hermes/kanban/boards/kami/outputs/requirements-gap-report/requirements-gap-report.md`, read-only gap analysis.
- **A** — `mvp-acceptance-criteria.md` in that same directory, proposed product-analysis criteria, including unapproved numerical limits.
- **T** — `architecture-options-tradeoffs.md` in that same directory, independent conditional recommendations.
- **P** — a new recommendation in this plan. Neither repetition of an earlier proposal nor inclusion here constitutes approval.

All three prior analyses were reviewed in full. R §§8–12 and D §§4–9, 11–19 supply the principal constraints. G's five release-blocking gaps remain open: identity lifecycle, delivery/task lifecycle, groups, adapter interoperability, and operational/release bounds. This plan does not resolve them by implication.

Revision basis: the five revised architecture drafts (`Communication_Architecture.md`, `P2P_Connectivity_Design.md`, `Relay_And_Offline_Delivery.md`, `Identity_And_Encryption.md`, `Messaging_Protocol.md` under `../architecture/`) and the available revised `First_Communication_MVP.md`, including AC-01–AC-10. The product J2 now correctly preserves logical ID/content binding, not identical transport ciphertext; architecture references to that earlier wording are reconciliation notes, not a new blocker. `Architecture_Review.md` R1–R6, the existing master roadmap/test plan, and the three historical evaluations (`../architecture/Language_And_Network_Stack_Decision.md`, `../architecture/Language_Stack_Independent_Review.md`, `Language_Implementation_Assessment.md`) were read as inputs. Their historical language recommendations are not the current selection and are not rewritten here. The roadmap and QA plan await their assigned downstream revisions. Product §4 excludes *claims of unverified relay admission/limits*, not the mandatory admission/limit approval and negative tests required by its checkpoints and the revised architecture; those remain pre-exposure gates here.

All workspace paths, interfaces, commands, schemas and dependencies below are proposed new surfaces, not existing Rust symbols or a verified manifest. No crate versions or target builds are validated. Downstream QA and manager reconciliation remain required; this plan does not preapprove their outcomes.

**P: first-message milestone (FM)** means two independently initialized CLI endpoints exchange authenticated, encrypted, one-to-one text, durably record local delivery, demonstrate direct connectivity where possible, and automatically use a self-hosted live relay when direct connectivity fails. A single-path slice is an internal precursor, not completion of FM. FM itself is not the full v0.1 MVP.

Full initial scope in R §10 additionally includes organizations/projects, invitations, mixed groups, encrypted offline mailboxes, artifacts, two harness adapters, and execution authorization. FM deliberately does not claim those capabilities. R's SHOULD requirements are not silently promoted to MUSTs by A's recommendations. Product approval must name the milestone and its exclusions before implementation starts.

**P assumptions requiring approval:** one device per participant for FM; manually exchanged, explicitly verified peer descriptors; no global public discovery; two human-operated endpoints; a named Linux development baseline and actual builds/runs on every additionally claimed OS/architecture; one self-hosted third-process relay with authenticated reservations and explicit pre-forward admission policy. Neither Linux packaging nor any additional target is proven. Participant/device separation is retained even in this restricted slice. These are temporary bounds, not a redesign of full identity or collaboration requirements.

## 2. Recommended minimal path and security boundary

**P:** use a Rust `kami` command-mode binary with clap and an in-process Tokio client engine, rust-libp2p TCP + Noise + Yamux peer connections, SQLite local state, and Circuit Relay v2 client support. A separate Rust `kami-relay` executable/process owns relay-server behavior. Begin with explicit verified address hints and one direct path; carry the same application protocol over an inner endpoint-authenticated relayed peer stream. Do not build a broker, GUI/TUI, daemon, public discovery/signaling service or second default transport in FM. QUIC/DCUtR require a separately approved NAT-profile investigation; direct TCP dialing is not general hole punching.

Noise in this proposal authenticates the expected remote Peer ID and protects a live endpoint-to-endpoint channel, including the inner peer connection through a circuit relay. A client-to-relay secure connection alone is insufficient: tests must prove the application stream terminates cryptographically at the intended peer, not the relay. Bind the authenticated transport identity to the locally verified device/participant record; never accept a display name as proof.

This is a deliberately limited online E2EE claim. Noise channel encryption does not produce independently decryptable offline mailbox envelopes, multi-device key distribution, encrypted local history, or group encryption. FM messages cannot later be uploaded as purported offline-E2EE ciphertext by reusing captured transport bytes. A reviewed asynchronous message-encryption protocol/library must be selected before C3; group crypto selection is a separate C4 gate. If the architect requires application-envelope encryption from FM, stop and reconcile that requirement before coding rather than inventing a construction.

The relay sees Peer IDs, addresses, timings, sizes, reservations, and connection relationships. FM provides neither anonymity nor protection against compromised endpoints. No received text invokes a shell, harness, tool, or approval action. Terminal control characters must be escaped in human rendering.

## 3. Proposed module boundaries and dependency direction

Propose one Cargo workspace with two binary crates and narrow internal library crates. This is a layout to create only after authorization, not a manifest written by this task. No crate is a promised external SDK or published package. Keep `pub` for tested cross-crate contracts, `pub(crate)` for implementation helpers and private modules by default; raw key bytes, SQL rows, libp2p objects and Tokio join handles are never public engine data.

| Proposed workspace path / crate | Owns | Must not own | Primary verification |
|---|---|---|---|
| `crates/kami-cli` / binary `kami` | clap parsing, process composition, profile/config and diagnostics modules, safe human/JSON rendering | Direct SQL or delivery algorithm | CLI golden/subprocess tests |
| `crates/kami-core` | Typed opaque IDs, logical message/receipt/fact/error types, pure delivery reducer and policy | Tokio, SQL, libp2p types, CLI strings | Property/reducer/fake-clock tests |
| `crates/kami-protocol` | Bounded framing, codec, version/field validation, wire↔domain conversion | Retry scheduling or trust approval | Golden vectors, parser fuzzing |
| `crates/kami-identity` | Key-provider port, protected-backend adapter, public descriptors, verification/pins | Account recovery invention or secret export | Proof/key-change/backend tests |
| `crates/kami-store` | Store port, SQLite worker, migrations, transactions, profile-lock support | Networking or interpretation of relay events | Crash/lock/idempotency tests |
| `crates/kami-transport` | Transport port and private rust-libp2p adapter; Swarm, sessions, stream codec integration, direct/circuit paths | User trust approval or delivered-state decisions | Adapter and real-path tests |
| `crates/kami-engine` | Public internal `ClientEngine` handle, delivery owner, retry scheduling and identity/store/transport composition | SQL/libp2p internals or terminal input | Deterministic async integration tests |
| `crates/kami-relay` / binary `kami-relay` | Independent Tokio relay server, operator profile/key, admission, limits and health | Client history, mailbox or endpoint decryption | Pre-forward denial/quota tests |
| `tests/fixtures`, per-crate `tests/`, CLI subprocess tests | Synthetic profiles, fake ports/clocks, bounded child-process harness and network manifests | Real keys/user data or implicit host-firewall changes | Reproducible fault/evidence suite |

Root `Cargo.toml`, `Cargo.lock` and `rust-toolchain.toml` are proposed future artifacts. Root `tests/fixtures` is data, not an implicit Cargo test target; put executable integration tests in the relevant crate or an explicitly declared test-support target. Do not create mailbox/group/adapter crates merely to reserve names.

Dependency DAG: `kami-core` is the leaf; protocol and identity depend on core; store depends on core; transport depends on core/protocol (verified identity expectations arrive as domain values); engine depends on the narrow identity/store/transport ports and core. CLI composes the concrete adapters. Relay depends on its own libp2p/operator support and only necessary common public identity/config utilities, not the client engine/store. Codec validation and security enforcement stay in library owners, not duplicated in binaries. Freeze each cross-crate port with its owning specialist before parallel implementation; avoid cycles and a catch-all shared crate.

## 4. Proposed stable interfaces and contracts

“Stable” means freeze these boundaries after approval and test compatibility; the names below are design notation, not existing Rust symbols or checked crate APIs. Commands return owned typed `Result` data with stable error code, retryability, safe detail and correlation ID. Tokio request/reply handles use bounded queues and deadlines; dropping a future never certifies rollback or nondelivery. Prefer concrete handles plus narrow ports over a blanket async trait hierarchy; select object-safety/`Send`/lifetime details when implementing the pinned toolchain. No borrowed key material or connection guard crosses an await boundary by accident.

| Interface | Proposed operations and result contract |
|---|---|
| `KeyProvider` | Create/load/use device key by opaque reference on bounded blocking backend; only identity/transport integration can obtain the library-required key handle/material under reviewed lifetime rules. No key export through engine/CLI/status. Backend and any required extractability chosen at G1. |
| `IdentityService` | `Initialize(profile)` → participant/device/public fingerprint; `ExportDescriptor()` → public descriptor; `ImportDescriptor(bytes)` → pending peer; `VerifyPeer(device, fingerprint)` → persisted pin/trust state. |
| `ClientEngine` | `Connect(peer, policy)` → authenticated session status; `Send(peer, text, request_id)` → stable message ID and local durability fact; `GetDelivery(message_id)` → per-device facts; `ListInbox(cursor)` → durable messages; `WatchEvents(cursor)` → replayable local events. |
| `PeerTransport` | Bounded `Dial`/`Listen`/`SendFrame`/`Close` commands; authenticated session/frame events include operation ID, expected/observed device binding, session and actual connection/stream ID, path and reason. All rust-libp2p types stay private. |
| `PeerSession` | Opaque session token refers to transport-owned streams; `WriteFrame` reports only attempt/write outcome, `ReadFrame` supplies bounded untrusted frame data with authenticated context. `PathInfo` is per frame/connection, never a peer-wide boolean or fabricated delivered fact. |
| `LocalStore` | Atomically reserve request/message IDs and outbox; commit authenticated inbox item plus dedup key and local event; persist authenticated receipt facts and reduce status; reserve/finish attempt; enumerate pending work. |
| `DeliveryPolicy` | Supplies direct budget, relay policy, end-to-end ACK deadline, attempt/byte caps, retry schedule and expiry from approved config; never derives authority from peer advertisements. |
| `Diagnostics` | Snapshot non-sensitive state and emit typed operational events with IDs/reason classes; bodies, keys and unrestricted local paths excluded. |

`Send` success at the library enqueue boundary means `local_outbox`, not delivery. CLI `--wait delivered` awaits a separate correlated delivery fact. `request_id` is a profile-scoped local idempotency token: atomically bind it to recipient/content and message ID; same token with changed content fails. On a canceled enqueue with unknown result, query that token before resubmitting. Frame validation produces a typed candidate; only verified session/trust checks can authorize an inbox transaction. A store result or generic transport event cannot manufacture an authenticated ACK.

### 4.0 Tokio ownership, backpressure and cancellation

- One process holds the exclusive profile lease throughout initialization/migration, engine runtime and store drain. Offline read commands also respect that lease in FM; no competing listener/reader connection model is promised. A separate relay profile has an independent lock and identity.
- One supervisor owns one Tokio runtime and tracks all tasks. One delivery task owns in-memory state/scheduling. One transport task exclusively owns and continuously polls the rust-libp2p Swarm, with a bounded number of owned stream-I/O tasks as required by the pinned API. It handles reservation renewal and connection/path events independently of foreground ACK waits. No SQL, keyring prompt, terminal read, blocking filesystem call or long CPU work runs on its poll path.
- Bounded command/event channels connect owners. Await queue capacity with a finite deadline or return explicit backpressure; never drop inbox/receipt/commit facts silently. Prioritize control/shutdown and drain stream results so a saturated application queue cannot indefinitely starve Swarm progress. Hard queue saturation closes/rejects bounded work safely and leaves durable outbox for reconciliation.
- SQLite uses one dedicated blocking worker/connection and a bounded queue. Key-provider calls use separately bounded blocking work with provider timeouts where supported. Tokio task cancellation cannot stop an already-running blocking transaction/provider call; supervise its result. Never spawn an unbounded `spawn_blocking` per message or hold a mutex/transaction across network awaits.
- Every queued operation has an operation ID and response channel. Cancel before dequeue may reject work; after transaction start, complete commit/rollback and reconcile by ID even if the caller disappeared. Delivery reservations/attempts interrupted by shutdown are recovered as uncertain, not successful. Inbound cancellation after inbox commit leaves the receipt fact available for re-ACK. Timeout of a frame write means it may have reached the peer; retry only the same ID/content.
- SIGINT/EOF stops admission of new sends, then requests cooperative task cancellation, finishes or reconciles in-progress store operations, closes streams/reservations and joins workers under an approved finite shutdown budget. If a blocking provider cannot drain, retain uncertainty and use restart recovery; never claim graceful durability because a task was aborted. Forced termination is covered by crash tests. Exited CLI engines do not retry in the background.
- Tests must cancel before queue admission, after enqueue, during SQL commit, during partial frame write and after inbox commit before ACK; stall store/key work while asserting reservation/Swarm liveness and bounded task/queue counts. Clock, transport and store fault seams are injected, not implemented by changing production trust policy.

### 4.1 Identity exchange

**P public descriptor:** schema version, participant ID, device ID, public identity key/Peer ID binding, key ID, public fingerprint, signed candidate addresses, optional relay address/Peer ID, creation/expiry information, and an authenticity proof defined by the approved identity protocol. A self-signature proves key possession, not a real-world identity. Verify the canonical signed representation, expiry, size limits, supported algorithms, and binding to the eventual transport peer before activation.

Import never trusts automatically and never changes a verified pin. Verification requires comparison over a separately trusted channel; noninteractive verification must supply the exact fingerprint explicitly. Address updates can retain an authenticated key binding, but a key change blocks new FM sends pending explicit reverification. Relayed connections are subject to the same rule. Bootstrap of the first trusted device is an explicit local action; it is not evidence of organization enrollment or account recovery. Do not invent a participant-root scheme to fill the missing lifecycle specification.

Proposed trust states: `pending` cannot send/accept private text; `verified` permits only the pinned device under the local FM policy; `changed` preserves the old pin and blocks new sends/inbound acceptance until explicit re-verification; `blocked` denies new sends and inbound acceptance and closes affected live sessions. Check trust at each send/receive, not only on initial dial. A durable local block cannot revoke keys or content already held by another endpoint and is not production revocation. Freeze block/commit races at G2 so no new acceptance follows a committed block. No automatic trust reset or TOFU is proposed.

Descriptors may contain private network metadata: share deliberately, bound address counts/lengths, and reject unsupported address families/protocols. Only dial deliberately imported destinations, never arbitrary addresses delivered in chat. Pin relay identity too; authenticate relay reservations and restrict access to the approved deployment's allowed devices. Administrative allowlisting is an FM deployment control, not a substitute for later tenant authorization.

### 4.2 Framing and message model

**P:** negotiate `/kami/chat/1.0.0` on an authenticated peer stream and use length-prefixed UTF-8 JSON frames. Both protocol ID and encoding are candidates pending G2 golden vectors, not verified rust-libp2p codec/stream APIs. Validate the prefix and finite length before allocation, then UTF-8, schema, required fields, identifiers, recipient, allowed types and absence of trailing/ambiguous data. G2 must independently approve plaintext text bytes and full encoded-frame bytes; G3 must reconcile those with circuit bytes including Noise/mux/framing/ACK overhead and retries. No earlier 256 KiB frame/payload candidate or inspected relay default is adopted. Reject oversize before enqueue/send; test below/equal/above each approved bound.

Proposed application fields: protocol major/minor, message ID, sender participant/device IDs, recipient device ID, event type (`chat.message` for FM), creation time, delivery expiry, and body. Identity fields must match the authenticated/pinned session; recipient must be the local device. Assign a random opaque ID from the standard cryptographic RNG before first transmission. Select exact ID encoding and entropy at G2; IDs are not timestamp-order guarantees. Same request/message ID always refers to the same content and recipient; content mismatch is a protocol violation, not a second message.

This FM application frame lives inside the secure channel and is not the finalized D §7 encrypted-envelope wire schema. Future mailbox routing headers and ciphertext envelope IDs are separately versioned. Never assume ciphertext is identical across recipients/devices.

Negotiate major version before sending text; incompatible major or required feature returns `protocol_unsupported` without payload processing. Optional unknown fields can be ignored only under the approved schema rules; unknown required fields/types are rejected. Select deterministic serialization for signatures independently of permissive JSON parsing; no signatures over ambiguous JSON bytes.

### 4.3 Durable receipts and state

**P:** keep durable facts, not one mutable “success” flag. FM sender facts are `local_outbox`, attempt observations (`connecting`, `in_flight`), and authenticated `device_delivered`, or terminal `delivery_expired`, `delivery_rejected`, `delivery_failed`. Path is separate metadata, not a message state. Every terminal failure has a named reason. `mailbox_accepted` is reserved for a future actual durable service receipt and is never emitted in FM.

A proposed device receipt carries protocol version, unique receipt/event ID, message ID, issuing/recipient device ID, sender correlation, durable-receipt event type, and emission time. Authenticate its issuer through the pinned session in FM and bind all identifiers to the outbox record. A future independently stored/forwarded receipt needs its own cryptographic authenticity construction. No claim of delivery from socket success, relay forwarding, or process exit.

Receiver checks bounds/version, session identity/trust, recipient and immutable-content consistency; for an unseen message it also checks approved expiry/skew rules. It transactionally stores inbox content, dedup identity, restart-stable receipt fact and the replayable inbox event. Only after commit may it emit `device_delivered`. A matching committed duplicate can return the prior receipt even after expiry, subject to current trust and retention policy, without another visible inbox item. Same sender/device/message ID with different content is rejected and diagnosed. A restart after commit but before ACK must recover the same logical receipt safely.

Reducer rules: duplicate receipts are idempotent; later nonterminal facts never erase confirmed delivery. Foreground timeout yields `delivery_unconfirmed`; expiry/attempt exhaustion records a local policy fact, not proof of nonreceipt. A valid late ACK preserves those facts and exposes `late_delivery_confirmed` rather than erasing uncertainty history or displaying another message. A contradictory authenticated permanent rejection cannot silently become success: retain both facts and stop for the G2-approved reconciliation rule. Freeze this distinction before code or fixtures; do not decide physical commit/expiry ordering using an untrusted remote clock. Task outcomes are independent and absent from FM.

Dedup records must outlive the permitted replay/retry horizon plus approved clock-skew allowance. Expired, unseen messages are rejected; known duplicates can return existing receipt facts. Retention cleanup must not re-enable replay. Exact skew allowance and dedup retention are G2 decisions.

## 5. Proposed CLI commands and observable behavior

All syntax below is proposed, not executable today. A global `--profile PATH` selects an isolated state root. `--json` selects versioned machine output. Profile lock contention returns `profile_in_use`; there is one state-owning live process per profile in FM, no daemon/IPC protocol.

| Proposed command | Observable contract |
|---|---|
| `kami init --name NAME` | Stages one local participant/device and protected profile, exposing ready state only after reconciliation/commit; prints IDs/public fingerprint only. Existing profile fails without overwrite; partial initialization is recoverable, never silent key replacement. |
| `kami identity show` | Shows local public identity and trust/storage status; never secret material. |
| `kami identity export --out FILE` | Explicitly writes a public descriptor with permission/privacy warning; no private key or history. |
| `kami peer import FILE --alias NAME` | Validates descriptor; stores pending/unverified peer. Duplicate identical import is idempotent; conflicting verified key is a visible blocked change. Alias is local UX only. |
| `kami peer verify ALIAS --fingerprint VALUE` | Requires exact comparison/confirmation and persists trust; mismatch fails without modifying pin. |
| `kami peer block ALIAS` | Commits local blocked trust state while holding the profile lease; next engine session refuses the peer. A running interactive engine needs an approved in-process block action for immediate session closure, not a competing profile process. No remote revocation claim. |
| `kami peer list` | Lists known devices, verification and last observed path; last observation is not live presence. |
| `kami listen --listen ADDRESS [--peer ALIAS]` | Runs inbound engine, receiver relay reservation/renewal and durable inbox/event rendering; optional peer accepts outbound bounded stdin lines. Bound-address disclosure only in explicit local discovery output under approved policy; diagnostics redact raw addresses. EOF/SIGINT triggers supervised drain/recovery. |
| `kami connect ALIAS --path auto\|direct\|relay` | Authenticates and probes a peer, reports verified identity, path and reason, then exits. Probe success is not message delivery and does not promise a reusable session after exit. |
| `kami send ALIAS --stdin --wait delivered --timeout DURATION --request-id ID` | Reads bounded text from stdin (avoids shell-history payloads), commits local outbox, tries delivery, and prints message ID/state. Command exit success requires authenticated durable-device receipt. An existing request ID returns the same message/status, not a new send. |
| `kami receive --since CURSOR --limit N` | Reads persisted local inbox after the live process exits; repeats are intentional history queries, not duplicate network delivery. No competing network listener is started. |
| `kami status [--message ID]` | Reads persisted identity/outbox/receipt/path observations and partial/late outcomes. Does not infer online status from cached data. |
| `kami doctor [--network]` | Checks permissions, key-provider access, configuration, schema and listener/relay reachability; network probes require the explicit flag. Returns redacted checks, never exports message bodies. |
| `kami-relay serve --listen ADDRESS --policy FILE` | Separate executable/process, separate operator profile/key; policy defines reservation and source→destination pre-forward admission plus finite connection/reservation/circuit/rate/byte/duration limits. No client profile access or mailbox promise. Exact policy format and enforcement API are G3 blockers, not a claim an allowlist flag is sufficient. |

Clap owns parsing/help/conflicting-argument checks only; engine/identity/protocol owners enforce semantic security and bounds independently of CLI parsing. Defaults and config precedence must be explicit, versioned and tested. Text comes from bounded stdin rather than message-bearing argv, environment variables or shell history; config/policy paths contain no inline secrets. `--timeout` cannot enlarge approved absolute lifetime or bypass shared attempt/circuit limits. Add finite `--limit`/cursor validation and help/usage/JSON snapshots.

For the two-live-process demo, receiver B runs `listen`; sender A runs `send` and exits. A separate `kami-relay` process is required for relay tests; it is not hidden inside either endpoint. For sustained bidirectional interaction with only two endpoint processes, each runs `listen --peer OTHER` and accepts outbound stdin lines or interactive input while receiving. The CLI and engine use the same Send API. Do not attempt a second `send` process against a profile already owned by `listen`. A future background daemon plus local authenticated API would be a separately approved architecture change.

**P output/exit contract:** stdout contains command data (one JSON result for bounded commands; NDJSON events for live commands), stderr contains redacted progress/errors. JSON includes `schema_version`, `operation`, correlation/message IDs, delivery facts, path, error code and retryability. No payload is included in diagnostics; receive output intentionally displays authorized local text. Escape terminal controls in human mode and JSON-encode untrusted text. Proposed exits: 0 = requested operation achieved; 2 = usage/configuration; 3 = identity/auth/policy; 4 = network unavailable; 5 = delivery unconfirmed/deadline; 6 = local durability/resource failure; 7 = incompatible protocol; 130 = interruption. Gate G2 freezes codes and output schema. A timeout after enqueue must print the stable message ID and `delivery_unconfirmed`; it must not suggest reissuing with a fresh ID.

A short-lived `send` process does not promise ongoing background retries after exit. Pending state survives; the next live engine session resumes within approved bounds. Status output explicitly reports `engine_not_running` and queued work when relevant. Interrupted processes retain committed records and never claim unsent/undelivered content vanished.

## 6. Direct/relay negotiation and bounded failure handling

**P automatic path algorithm:**

1. Validate verified identity, address policy, pending outbox record and remaining delivery budget.
2. Reuse an authenticated suitable session if present. Otherwise attempt supported direct candidates under a bounded direct budget; candidate count and concurrent dials are bounded.
3. On direct timeout/unreachability, try the configured authenticated relay reservation/circuit within the remaining overall deadline. Establish the inner secure session to the expected endpoint before opening the Kami stream.
4. Record selected actual connection path from transport metadata, not from a supplied `--path` label. Emit a safe reason (`direct_unreachable`, `direct_timeout`, `relay_selected`).
5. Send the same message ID and await device receipt. If the session drops or ACK is lost, retry on an eligible path with identical logical identity; receiver commits/displays once.
6. Retry direct optimization only outside the critical send or on later connections. Keep a working relay session until replacement is authenticated; no disconnect-first migration.

`--path direct` and `--path relay` are explicit diagnostic restrictions; `auto` is the user default and the fallback acceptance path. Restricting direct traffic in a test must be real transport/network fault injection, not just setting relay-only mode. Lack of reservation, relay quota or relay outage is not recipient-offline proof. A remote identity/authentication mismatch is a hard failure, not a reason to try a weaker channel.

G2/G3 must approve finite direct-dial, handshake, stream-I/O, ACK-wait, foreground, shutdown and absolute-delivery budgets separately, plus candidate/concurrency, retry/backoff/jitter, attempt and byte caps. No earlier timing/TTL/attempt proposal is adopted here. Count actual frame-send attempts across paths toward one shared cap and track dials separately; direct work cannot consume the entire fallback budget. Reservation renewal and relay denial are bounded, not infinite retry loops. Use monotonic elapsed time for waits and an approved restart/clock-skew rule for persisted absolute expiry.

A §2/§5 proposes 10 attempts and 24 hours, but starts mailbox TTL at mailbox acceptance while local attempts already consume time. **P:** define a sender absolute expiry at local enqueue, never extended by retry, and later use mailbox expiry no later than that deadline. This differs from a fresh 24-hour post-acceptance TTL and must be reconciled before C3. Retry exhaustion before expiry is a named visible outcome; no infinite silent queue. Receipt-wait timeout alone leaves a pending/unconfirmed message, not a new logical send.

| Failure | Required proposed observation and handling |
|---|---|
| Invalid/changed peer credential | Block send; `identity_unverified`/`peer_key_changed`; explicit verification, no downgrade |
| Wrong remote identity or bad receipt issuer | `authentication_failed`; close/quarantine session, no delivered transition |
| Bad/oversize/version-mismatched frame | Reject with bounded safe error; no inbox commit, no delivered ACK; limit abusive peers |
| Direct blocked, relay healthy | Auto chooses authenticated relay inside approved budget; same ID and eventual device receipt |
| Relay disconnects mid-frame/ACK | Preserve pending outbox, reopen eligible path, retry within shared bounds; one inbox item |
| Receiver not reachable and no mailbox | `delivery_unconfirmed`/`peer_unreachable`; local pending only, never `mailbox_accepted` |
| Both relay and direct fail | Visible reason classes and remaining policy; retain durable outbox until explicit terminal outcome |
| Sender/receiver crash | Resume committed pending state; receiver re-ACKs committed duplicate; uncommitted work never ACKed |
| Disk full/store commit failure | `local_store_unavailable`/`local_outbox_full`; no transmission before sender enqueue, no recipient ACK before commit |
| Deadline/attempt cap exceeded | Visible expired/failed-unconfirmed reason and ID; warn delivery may have occurred if ACK lost |
| Profile locked/key provider unavailable | Fail before network operation; no regenerated key, no plaintext fallback |
| SIGINT/process termination | Bounded graceful shutdown where possible; restart recovery for committed state, no success synthesized |

## 7. Dependencies and selection rationale

Rust is selected; every dependency/configuration below is still a candidate requiring architecture/security approval, exact pinning, license and vulnerability review, and actual supported-target builds/runs. No versions are verified or installed by this task. Historical upstream observations are not current compatibility evidence; a documentation-build failure neither selects nor disqualifies an application dependency.

| Concern | Proposed Rust component | Required evidence / boundary |
|---|---|---|
| Runtime | Tokio; futures utilities only where the selected libp2p stream API requires them | One runtime per process; explicit I/O/time/sync/signal features; prove compatibility, bounded queues, cancellation and polling liveness. Do not assume every stream implements Tokio I/O directly. |
| CLI | `clap` command-mode parser | Pin selected derive/help features; golden help/usage/exit/JSON tests; semantic authorization stays outside parser. No TUI dependency. |
| Live network | `libp2p` from rust-libp2p with TCP, Noise, Yamux, relay client/server and necessary behavior/stream support | Verify exact builder/behavior/stream APIs, feature names and inner-session identity on the pinned build. Identify/DNS only if needed and approved; no implicit QUIC, DCUtR, public DHT, pubsub or extra listeners. |
| Addresses | libp2p-compatible multiaddress/Peer-ID representation private to transport/identity adapters | Pin dependency/re-export compatibility; bound count, lengths and allowed components. Public core uses opaque validated IDs, not foreign library types. |
| Framing/config | `serde`/`serde_json` for bounded schemas and versioned nonsecret configuration | Bound before deserialization/allocation, reject ambiguity/trailing data and unknown required fields; JSON alone is not canonical signing. |
| Local SQLite | `rusqlite`, with bundled SQLite only if approved | Single blocking worker, parameterized SQL, explicit transactions/PRAGMAs and migration/recovery tests. Bundled SQLite involves native compilation/linking: Rust-only application code is not a pure-Rust dependency graph or static-binary guarantee. |
| Keys/history protection | Reviewed Rust key-provider adapter; approved encrypted store or explicitly bounded encrypted-volume lab profile | No backend/version currently validated. Prove headless/locked-provider behavior and library identity-key requirements. SQLCipher, if required, needs a reviewed provider/feature/native-link/key-delivery build before real private data; stock SQLite does not encrypt DB/WAL/backups. No plaintext-key fallback or improvised field encryption. |
| IDs/diagnostics | Reviewed OS-backed secure RNG/ID crate after G2 format choice; `tracing` with safe-field subscriber | No deterministic upstream example keys. Never derive IDs from names or unsafe seeds. Audit dependency debug output as well as application fields; no bodies, keys or raw addresses by default. |
| Test tooling | Rust unit/subprocess tests; candidate property testing and `cargo-fuzz` tooling after approval | Dev-only dependencies isolated from distributable features. Optional Loom/Miri have scoped compatibility limits; neither proves arbitrary Tokio/libp2p/native behavior correct. |
| Later async/group crypto | No selected dependency in FM | Separate reviewed Rust protocol/provider and state-recovery gates before mailbox/group use; do not include HPKE/MLS simply because a crate exists. |

Future dependency policy: centralize reviewed versions under workspace dependencies, pin a Rust toolchain/MSRV/edition compatible with those actual builds, retain `Cargo.lock` for binaries, and record registry/source provenance, transitives, licenses and native linkage. Avoid unpinned Git branches and wildcard recommendations. Disable unneeded default features only where supported, enumerate required features and inspect the *effective* union using `cargo tree -e features`; workspace feature resolution does not imply isolation of every dependency. Select the Cargo resolver explicitly when the toolchain/edition is approved. No `full` feature shortcut, sensitive crypto/content debug feature, unused transport or network-discovery feature belongs in release profiles.

Plan locked build/test/Clippy checks on each supported feature/target profile; do not use `--all-features` as proof of the minimal release graph. Record build target, linker, native SQLite/crypto/key-provider dependencies and actual headless behavior. Dependency updates require compatibility/security review and rerun of affected golden, identity, circuit, crash and packaging tests. Application crates should avoid unsafe code; any necessary FFI stays encapsulated in reviewed providers and does not imply native dependencies are absent. Package both executables with separate config/profile permissions and operator instructions only after target run/install tests; no packaging, signing, static linking or performance claim is verified here.

Protocol references for future pin review: libp2p Noise specification (<https://github.com/libp2p/specs/blob/master/noise/README.md>), Circuit Relay v2 (<https://github.com/libp2p/specs/blob/master/relay/circuit-v2.md>), and the upstream links in the three historical evaluations. They were historical documentary inputs, not fresh API/build validation for this revision. The bounded experiments in §10 must resolve actual API availability.

## 8. Data ownership, storage, and process boundaries

**P profile layout:** versioned nonsecret configuration; public identity and peer trust records; SQLite state; opaque key-provider references; process-lock metadata; optional redacted operational logs. Explicit profile paths take precedence over the eventual platform default. Directory permissions must restrict local access; secret material must not appear in exports, command arguments, ordinary config, logs or diagnostics. Check symlink/permission hazards during initialization and writes.

Proposed database responsibilities:

- Identity/peer rows: public participant/device/key binding, pin, verification method/time, pending changes; never recover an identity by reusing a name.
- Outbox rows: immutable message/request identity, recipient, protected payload, creation/expiry, content binding and durable enqueue fact; separate attempt rows record path and safe errors.
- Inbox rows: sender/device/message unique key, protected content and durable receipt facts; atomic inbox-event append enables restart-safe subscription cursors.
- Receipt facts and delivery projections: authenticated issuer, message correlation and event uniqueness; projection rebuild must preserve historical contradictions rather than lose facts.
- Versioned migrations: exclusive profile ownership, transactional upgrade when possible, explicit unsupported-newer-schema rejection, backup/recovery procedure that does not expose plaintext.

### SQLite transaction and crash contract to freeze at G2

Propose WAL journal mode with `synchronous=FULL`, foreign-key enforcement, a finite busy timeout and a local filesystem whose locking/sync guarantees are supported and tested. These settings are proposals, not verified durability or permission to weaken sync for performance. Check effective PRAGMA results on open; reject unsupported modes/targets. One worker owns the connection; no transaction spans an await/network operation. WAL checkpointing is maintenance, never the receipt boundary. Power-loss/media guarantees depend on filesystem/device behavior and require separate evidence; process-kill recovery alone does not establish them.

| Atomic operation | Proposed committed records / postcondition | Crash/cancellation behavior |
|---|---|---|
| Initialize/migrate | Schema version, local public identity/key reference and profile-ready marker under exclusive lease | Filesystem/key provider/SQLite cannot share one transaction. Stage initialization, keep an explicit incomplete marker, reconcile provider-created key or orphan on restart; never silently replace identity. Migrate transactionally where possible, fail safely on newer schema, and approve protected backup/restore first. |
| Enqueue | Unique local request mapping + immutable message/recipient/content binding/deadline + outbox fact/event | Before commit: no network send. After commit: queryable pending ID even if reply is lost. Reuse request ID, not a new message, on uncertainty. |
| Reserve/finish attempt | Persistent attempt identity/count/path and outcome observations | On restart, incomplete attempts are uncertain; consume/reconcile under approved cap, never synthesize ACK or restart an unlimited budget. |
| Accept incoming | Unique `(sender_device_id, message_id, recipient_device_id)` + immutable content comparison + inbox + restart-stable receipt fact + one replayable inbox event | No ACK before commit. Matching duplicate reads prior receipt; changed-content duplicate fails `id_conflict`. Crash after commit/before ACK is recovered by same-ID re-ACK without new logical event. |
| Accept receipt | Validated issuer/correlation + unique receipt fact + reduced sender projection/event | Commit before CLI success; lost caller reply is recovered by ID. Duplicate ACK idempotent; late confirmation preserves local timeout/expiry facts. |
| Retention | Approved payload cleanup with durable replay-safe dedup/receipt tombstones | Never purge dedup before full retry/late-arrival/skew horizon or silently delete unacknowledged outbox to free space. Cleanup must not turn a committed duplicate into new content. |

Storage-full, failed sync, busy-timeout, corruption, unavailable protected key and migration failure produce explicit non-success without send/ACK for the uncommitted operation. If commit outcome is ambiguous, reopen/reconcile by durable operation/message key before retrying. Profile-lock recovery must use actual OS lock ownership rather than treating a stale PID file as authority. Database/WAL/shm/temp/backup files and key references obey the approved protection policy; parameterized SQL and validated paths are mandatory. Recovery tests must kill before/after each commit boundary, inject disk/sync faults where supported, replay duplicates/receipts, and check acknowledged facts after restart.

One durable inbox event is not an exactly-once terminal-write guarantee: a crash between output and recording a render cursor can cause replay. Network retries append no second logical event; replayed history/live events retain the same cursor/ID and are labeled as replay where applicable. G2/QA must approve render-cursor semantics and distinguish deliberate history queries from duplicate network display, rather than promise atomicity between SQLite and a terminal.

At-rest storage policy is a G1 blocker, not an implementation afterthought. For an explicitly authorized lab preview only, an encrypted-volume profile with disclosed plaintext SQLite semantics may be considered; full local-at-rest encryption must not be claimed for it. If G1 requires application-managed encryption, implement only the approved reviewed backend before storing real private text. SQLite journal/WAL, temporary files, backups and crash dumps are in the confidentiality review, not just the main database.

The relay owns its infrastructure key, admission policy, reservation metadata and bounded ephemeral buffers. It does not own client inboxes or persist text as an offline queue. Durable mailbox data, when approved later, belongs to a separate service contract: ciphertext, minimal routing/expiry/quota metadata and authenticated receipts, never endpoint plaintext or private keys.

Resource control: enforce frame/text bounds, peer/address count, pending attempts, outbox/inbox bytes, log rotation and dedup retention. A §5's 1 GiB outbox is a proposal; inbox retention and replay-safe dedup cleanup remain unspecified. G2 must approve finite values and predictable full-store behavior before FM acceptance. Do not delete unacknowledged content silently to meet a quota.

## 9. Local two-process development and integration strategy

**P exact development model:** two endpoint processes, A and B, with separate temporary profile roots, key-provider namespaces, databases, locks, configured listen ports and public descriptors. Test payloads are harmless generated fixtures. No shared identity files or shared database; no private key exchange. The future harness creates/removes only its owned disposable profiles.

1. Initialize both profiles independently; export/import public descriptors and compare fingerprints through the test's trusted control channel. Confirm importing alone cannot send. Verify both inbound and outbound session identity/device binding and relay pinning, not merely the sender's UI trust label.
2. Start B's listener on a loopback ephemeral port, discover actual bound address, and start A's sender with B's verified descriptor. Require a durable receipt, inspect both local records and display events. Repeat in reverse using A as listener, or use two interactive endpoint sessions for simultaneous bidirectional chat.
3. Kill B after inbox commit but before ACK; restart B, retry the original A message, and assert one inbox item/display event plus recovered receipt. Kill A after enqueue and show that pending work resumes without a fresh ID.
4. Test address-in-use, malformed/oversize frames, wrong peer pin, version mismatch, lost ACK, disk-full/store fault, bounds, EOF and interrupted shutdown with injected dependencies and bounded processes.
5. Add a separately running relay fixture; register B's reservation before attempting A→B, with outbound connectivity from both endpoints. Force A's direct route to fail at the transport/network boundary and leave relay route usable. Require `auto` fallback and endpoint-to-endpoint authentication.

A real relay scenario necessarily has infrastructure beyond two endpoint processes. Do not describe an in-memory fake or a relay hosted by one endpoint as independent cross-network fallback evidence. The preferred integration test has two clients plus a third relay process (or an explicitly already-running self-hosted fixture). A namespace/firewall fixture may also need elevated test-environment permissions; arrange those separately rather than changing the operator workstation firewall.

Loopback does not establish NAT traversal, CGNAT support, enterprise egress compatibility or cross-network success. Follow with isolated network-namespace tests and two real hosts on different networks, recording NAT/egress conditions. Explicit candidate exchange means no rendezvous/discovery claim. Hole punching/AutoNAT/DCUtR are later evaluated optimizations, not guaranteed to work everywhere. TCP itself can be blocked; if supported environments cannot reach the relay, report failure and revisit transport/deployment assumptions instead of claiming universal fallback.

## 10. Phased work packages and exit gates

No delivery schedule, implementation authorization, Jira objects, branches or new tasks are created by this plan. Logical owners are suggested handoffs for later assignment; the feasibility stop budget below is also pending approval.

| Phase / work package | Dependencies and proposed owner | Exit evidence / stop gate |
|---|---|---|
| P0: scope and contracts (D C0) | Product + architect, QA review | Approve G0–G2; freeze trust ceremony, wire/receipt/error contracts, state table, limits, key protection and milestone claims. Stop on conflicting sibling drafts; language is already Rust, not a new comparison decision. |
| F1–F3: bounded Rust feasibility | P0 + G3-approved trial topology/bounds and separately authorized experiment permissions/budget; senior networking/storage and QA | Run the checkpointed trials below before committing to API/packaging/network capability. The trial creates only a disposable minimal workspace; P1 creates the maintained implementation after review. No prototype promotion or production deployment by default; stop on a mandatory failed gate. |
| P1: workspace/profile/identity/storage | P0 + feasibility gates; developer with senior security review | Create approved Cargo workspace/lock/toolchain and minimal features; staged init/re-init refusal, isolated identities, verified peer import, protected key storage, durable store migrations and injected failure tests; no plaintext fallback. |
| P2: first secure direct text | P1; senior networking + CLI developer | Two endpoints, authenticated direct stream, stable IDs, durable inbox before ACK, one logical display across retry/restart; safe CLI JSON/exit behavior. Internal single-path precursor only. |
| P3: real live relay and automatic negotiation | P2 + G3; senior networking, QA fault matrix | Separate relay reservation/admission; blocked direct under auto policy delivers via end-to-end secured relay; migration/ACK-loss dedup; no silent ID change or downgraded auth. |
| P4: FM integration and review | P3; QA + evaluator | Reproducible direct/relay/failure matrix, network conditions and measured timings, privacy inspection, portability evidence for any claimed OS; G4 review and owner approval. FM stops here, not v0.1 release. |
| P5: async E2EE, discovery, tenant auth and mailbox (D C3) | FM + approved identity lifecycle/authorization/async crypto contracts | Current authorization at submission/fetch/direct acceptance, ciphertext durable queue/expiry/quota and authenticated ACKs; restart and revocation tests. No queue acceptance conflated with delivery. |
| P6: groups and artifacts (D C4) | P5 + group membership/crypto/ordering decisions | Authoritative recipient snapshots, per-device ledger, join/remove and missed-epoch behavior; encrypted artifacts with independent access/integrity checks. Resume only if selected, not elevated silently from SHOULD. |
| P7: two adapters (D C5) | Task lifecycle/policy + P5/P6 capabilities; adapter specialists | Named harness/version/platform matrix, real bidirectional non-destructive tasks, capability/approval/idempotency/cancellation negatives; no A2A conformance claim without separate evidence. |

Critical path: approved scope/trust/storage/receipt contract → durable direct path → real relay negotiation → FM verification. Storage and CLI rendering can be worked independently only after interfaces are frozen. Do not parallelize multiple edits into the engine/protocol schema without a designated owner; those become integration hotspots. Later mailbox/group/adapter phases require their own approvals and assignments, not scope expansion within FM.

### Bounded Rust-only feasibility trials (planned, not executed)

Proposed cap: five engineer-days total, excluding host provisioning and owner review, with a checkpoint after each trial. This is a stop/review budget requiring owner approval, not an estimate, schedule or authority to experiment now. Predeclare finite message counts, frame/circuit sizes, deadlines, platform list and network fault permissions with QA; no open-ended load test, full second implementation, new crypto or library fork. Reuse one tiny frame/ACK fixture and disposable owned profiles; retain failure evidence.

| Trial | Smallest artifact and assertions | Exit/stop |
|---|---|---|
| F1 pinned composition/admission/packaging | Rust toolchain/Cargo lock/features; minimal TCP/Noise/Yamux endpoint and separate relay processes; bounded app frame/ACK; exact verified device↔Peer-ID binding, wrong/changed key and relay impersonation negatives; reservation renewal; deny unauthorized reservation and source→destination circuit *before useful forwarding*; circuit bytes/duration/rates include secure/mux/ACK overhead. Build and execute on each claimed target; inventory native dependencies/key backend. | Exact API/feature/build/config and admission proof, or stop. Inspected Rust relay configuration does not establish an application ACL hook; post-accept events or a generic connection allowlist are not proof. Missing mandatory admission/packaging/key support returns to architect/operator; no public open relay or unapproved fork. |
| F2 durable protected async seam | Add minimal SQLite outbox/inbox/dedup/receipt, staged init, exclusive profile lock, chosen key/history backend. Cancel and crash before/after commits and after receiver commit before ACK; same-ID retry/content conflict, late ACK, store-full/unavailable-key, saturated queue and delayed store/key worker. | Outbox before send, inbox before ACK, one logical event/receipt, honest uncertainty, bounded workers and Swarm/reservation liveness under the approved budget. Stop on rollback assumptions after cancellation, plaintext fallback or unbounded queues/tasks. |
| F3 real direct and automatic fallback | Same two endpoint builds on independently recorded networks plus third relay process. Direct-eligible frame **and ACK** actually use direct connection. Inject direct failure without `--path relay`, preserve both outbound relay paths/reservation, use `auto`, retry same ID over inner endpoint-authenticated circuit. ACK loss/restart, reservation expiry/quota/outage/no-route negatives and redacted sentinel inspection. | Both actual direct and genuine auto fallback evidence, identity/receipt/order/caps/timing and single inbox event. Loopback, a ping, forced relay-only or peer-wide connected flag is insufficient. Unsupported profiles or missing direct evidence fail/stop the FM feasibility claim; no alternative-language proposal. |

If an approved NAT profile demands hole punching, obtain a separate bounded Rust DCUtR trial over an existing relayed connection; QUIC requires its own TLS/identity/UDP and limit configuration. Keep relay usable on failed upgrade. Report direct-first reachability separately from relay-assisted direct upgrade; Product decides which meets the named criterion. HTTPS-only proxy support is unverified; TCP/443 does not imply proxy compatibility. A passing F1–F3 is feasibility evidence, not full FM qualification, production readiness or authorization for later packages.

### Module handoffs after contract freeze

- Architecture/security → identity/protocol owners: approved trust-state table, descriptor/proof vectors, Noise/device binding, key/history policy, framing/receipt/version/replay rules; no assumed crate callback or invented symbol.
- Core/protocol → store/transport/engine: typed message/ACK/errors, immutable-content rules, golden bytes, reducer table including late ACK/permanent-rejection contradiction, finite bounds and clock semantics. Core/protocol contract changes have one designated owner and compatibility review.
- Store → engine/QA: migration/schema version, unique keys and transaction boundaries, operation-ID reconciliation, sync/filesystem assumptions, quota/retention and profile-lock semantics, crash fixtures. Engine never implements SQL recovery independently.
- Transport/relay → engine/operations/QA: pinned feature/API profile, authenticated frame/ACK context and actual connection/path IDs, pre-forward reservation/circuit denial proof, configured overhead budgets, renewal and no-route taxonomy. No delivered event from relay or write completion.
- Engine → CLI/QA: enqueue-versus-delivery result contract, bounded queue/cancellation/shutdown behavior, restart and inactive-pending statuses, replay cursor, safe events and injected fault seams. CLI owner implements clap/JSON/help/exit snapshots, not another reducer.
- Each handoff includes contract version, scoped files/crates, tests/evidence actually run, known unsupported targets/profiles and next approval gate. QA independently crosswalks AC-01–AC-10; manager reconciles remaining roadmap/design guidance. These are future assignments, not additional tasks created here.

## 11. Verification matrix and integration seams

All checks below are planned evidence, not reported passing tests. QA owns reconciliation with its independent test-plan draft. A reproducible record includes safe build/config versions, pseudonymous devices/message IDs, actual selected paths, fault timing, state facts, duration and pass/fail; payload/keys are omitted from operational artifacts.

| Gate | Minimum observable FM evidence |
|---|---|
| Initialization/identity | Independent IDs/keys; re-init cannot overwrite; unverified or mismatched peer blocked; exact verification unlocks only the approved peer |
| Direct send/receive | Verified endpoint session; sender durable outbox precedes write; receiver durable inbox precedes ACK; sender delivered only after authenticated receipt |
| Fallback | Direct made genuinely unavailable; `auto` chooses a separate healthy relay within approved deadline; stable logical ID; receiver authenticates A rather than relay as sender |
| Migration/retry/restart | Duplicate direct/relay arrivals and lost ACK yield one inbox/event item; process crashes at persistence boundaries recover committed state; late receipts do not fabricate/erase outcomes |
| Diagnostics/security | No payload/key sentinels in relay state/logs or diagnostics; malformed/oversize input bounded; control characters rendered safely; profile/key/storage faults explicit |
| Offline honesty | Stop receiver with no mailbox: pending/unconfirmed or named terminal failure only, never server-accepted/delivered; no implied background retry after sender exits |
| Bounds and versions | Exact/over-limit cases; unknown required features and major-version mismatch rejected before payload processing; retry/deadline and dedup-retention boundary tests |
| Deployment realism | Loopback and isolated fault tests plus two-host network evidence; record egress restrictions and unsupported conditions; do not infer internet-wide connectivity |

Use unit/property tests for reducers/validators/policies, golden tests for frames and clap/CLI output, bounded parser fuzzing, transactional/crash tests for SQLite, deterministic Tokio cancellation/liveness tests and actual subprocess tests for both binaries. Future checks after the workspace exists: `cargo fmt --all -- --check`, `cargo test --workspace --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo build --workspace --locked`, and `cargo tree -e features` for approved profiles. Test named targets/features explicitly; these are proposed commands, not execution results. Approve/install any fuzz/property/security tooling separately. Time-box fuzz jobs with corpus/seed retained; use Loom only for modeled synchronization seams and Miri only for supported Rust/unsafe-boundary cases, not entire native/network stacks. Review dependency advisories/licenses using approved tooling and inventory native libraries. No checks in this paragraph have run in this documentation task.

A's proposed full-MVP bounds (10 participants/20 group devices, 20 concurrent devices, 100 MiB artifacts/4 MiB chunks, 500 MiB mailbox per device/10 GiB per organization, 24-hour TTL, 95/100 per-path success, online p95 ≤5 seconds, offline catch-up 30 seconds, and 60-second healthy-deployment revocation) remain proposals for later owner approval and measured validation, not FM criteria by inheritance. FM acceptance must set its own sample count, success criterion, fallback/ACK bounds and network assumptions at G3; report observations without invented SLA claims.

QA crosswalk: AC-01 → identity/key/profile and inbound/outbound binding; AC-02 → enqueue/ID/content transaction; AC-03/04 → actual separate-network direct and auto fallback frame/ACK; AC-05 → declared relay/log/buffer inspection plus separate security review; AC-06 → duplicate/path race/ACK-loss restart; AC-07 → receipt-gated CLI states; AC-08 → identity/auth/parser/no-route/relay/store/cancellation negatives; AC-09 → correlated safe events, terminal escaping and library-log redaction; AC-10 → exact build/features/config/fixtures/topology/fault/evidence manifest. A case passes only with every assertion and evidence item; missing approved contract/environment is `not testable`, failed fault setup is `invalid`, later mailbox/group/artifact/task tests are `excluded for FM`, not passes. Preserve failed runs and limitations.

Integration seams to preserve: public-key binding independent of display name; recipient-device addressing independent of current transport address; `ClientEngine` reused by future authenticated local adapters; immutable logical message IDs independent of per-device encrypted envelopes; explicit receipt issuer/durability level; policy independent of advertised capabilities; artifact access independent of transport; storage migrations and schema negotiation independent of CLI output version. The future task API reserves `task_id` and optional `attempt_id` separately from `message_id`; no current chat input becomes execution authorization.

## 12. Risks, unresolved choices, and approval checkpoints

| Risk / unresolved decision | Consequence and proposed mitigation | Owner gate |
|---|---|---|
| FM confused with full v0.1 | Misleading delivery/groups/adapter promises; approve explicit milestone wording and exclusions | Product G0 |
| Identity bootstrap, recovery and stale authorization unspecified | Impersonation or revoked direct peer access; restrict FM to verified pins, no recovery claim, require authoritative revocation model before production/tenant use | Product + security architect G1 |
| Headless keyring or local-at-rest backend unavailable | Unsafe plaintext fallback; stop and select approved reviewed alternative, including backup/WAL treatment | Security architect G1 |
| FM session E2EE mistaken for mailbox encryption | Server could later receive plaintext or unusable captured stream bytes; separate async protocol/library gate | Architect G1 and P5 |
| Protocol/delivery terminal rules conflict with earlier proposals | Inconsistent send status and unsafe retries; approve transition table including late confirmation and content-ID mismatch | Architect + product G2 |
| Numeric retry/expiry/frame limits conflict across drafts | Unbounded queues, lost fallback budget or interoperability failure; reconcile units, start times, attempt counting, retention and schema | Architect + QA G2/G3 |
| Relay admission API unverified | No proven supported pre-forward reservation and source→destination authorization; deny useful forwarding in negative tests, no assumption of callback/fork or tenant policy from connection allowlist | Architect + operator G3/F1 |
| Frame/circuit budgets unverified | Encoded frames, secure handshake/mux/ACK and retries may exceed circuit byte/duration/rate limits; approve separate units/overhead and boundary/renewal/mid-stream tests | Networking owner + QA G2/G3 |
| Relay/NAT behavior unverified | Direct TCP may not traverse two private NATs; reservation, outbound egress or relay failure prevents fallback. Require three-process separate-network evidence, actual per-frame path and unsupported profiles; DCUtR/QUIC only separately gated | Networking owner + QA G3/F3 |
| Rust crate APIs/features/platform compatibility unverified | Swarm/stream/relay/key-provider/SQLite integration may not match examples; pin actual APIs/features and build/run profile before capability claims | Architect G1/G3/F1 |
| Packaging/native dependencies unverified | Bundled SQLite, encrypted-store/key backends or target linkers may fail; inventory linkage/licenses, run both binaries and key/storage lifecycle on every claimed target; no static/pure-Rust/cross-platform promise | Maintainers + QA G3/F1 |
| Tokio cancellation/worker liveness | Dropped future mistaken for rollback or blocked Swarm loses reservation; bounded owners/queues and operation-ID reconciliation; cancel/commit/backpressure/shutdown tests | Senior developer + QA G2/F2 |
| Shared profile/process ambiguity | Lock contention or unreliable concurrent SQL/network owners; one live owner per profile, no implicit daemon | Senior developer G2 |
| Diagnostics/metadata leak | Text, control sequences, key material or private network data escapes; safe-field allowlist, output escaping, sentinel inspection and operator retention rules | Security reviewer + QA G4 |
| Manual descriptors diverge from permitted discovery requirement | Prototype works but does not satisfy ID-04 or tenant isolation; explicit partial-coverage declaration, permission-aware discovery at P5 | Product G0/P5 |

Approval checkpoints:

- **G0 — product scope:** approve FM label, non-goals, actors/device restriction, supported deployment/platform claim and separation from v0.1. No implicit requirements amendment.
- **G1 — security/architecture:** Rust is selected; approve exact live-E2EE configuration, bootstrap/verification/key-change/block behavior, private-key backend and local storage confidentiality policy. Full key lifecycle remains a production gate even if advanced recovery is deferred.
- **G2 — contract freeze:** approve exact IDs/frame and descriptor schemas, receipt authentication and transition table, CLI/JSON/exit contracts, quotas, retry/expiry semantics, clock skew and replay-safe retention. Record normative versions before teams implement independently.
- **G3 — networking/test readiness:** approve relay admission/config/limits and network matrix, dependency pins, direct/overall timeout bounds, FM measurement sample/thresholds and fault-injection permissions. Stop if a required environment cannot reach the relay or a reviewed crypto/storage choice is infeasible.
- **G4 — FM acceptance:** evaluator reviews product/architecture/implementation/QA drafts together; later actual implementation requires independent review and recorded test evidence. No deployment or release claim on document approval alone.
- **G5 — full-scope progression:** owners close G's five decision packages before the corresponding P5–P7 implementation/release claims. Choose authoritative membership/ordering, crypto library and device lifecycle; name two harnesses and A2A posture; approve deployment limits and measurable operations. A's numbers cannot become normative through a planning reference.

Handoff: this revision of `t_295267ab` releases pre-created QA task `t_5455f2bf` and contributes to manager reconciliation task `t_a1c3f960`, with their other dependencies still applying. QA must use the revised architecture/product and this proposed Rust workspace/async/storage contract; manager reconciles historical language guidance and approval status without rewriting historical evaluations. Finishing this planning card does not certify implementation, approve the remaining technical selections or authorize Git/Jira operations. Subsequent approved implementation follows normal Jira tracking, reviewed branches/PRs and evidence gates under AGENTS.md, with no direct-to-main push or unauthorized merge. This task writes only this document and creates no additional work items.
