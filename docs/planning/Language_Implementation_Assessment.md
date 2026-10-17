# Kami Language Implementation Assessment

Status: Draft — Pending Approval

Planning task: `t_3e5b3cbc`
Disposition: bounded feasibility investigation recommended; no language, dependency version, implementation, or security claim approved.

## 1. Executive assessment

**Recommendation:** investigate **Rust + Tokio + rust-libp2p + endpoint-to-endpoint Noise over TCP/Yamux + Circuit Relay v2**, with a transactional SQLite local store, as a practical candidate for the online first-message milestone (FM). Include QUIC/DCUtR in the connectivity experiment when the approved network profiles require NAT traversal; do not promise that TCP address exchange alone establishes direct connectivity between two private NATed endpoints. Retain **Go + go-libp2p** as the comparison baseline and lower-integration-complexity alternative. Do not select C or C++ for the whole engine without a concrete native-library or team-expertise constraint that outweighs its integration and memory-safety costs.

This is a recommendation to collect evidence, **not a conclusion that Rust is faster to deliver than Go**. The previous roadmap's Go preference is defensible for the narrow online slice; no inspected FM requirement forces a language change. Rust is credible because its upstream networking examples already compose the relevant secure direct/relay and hole-punching components, and it has a native MLS candidate for later investigation. Neither upstream examples nor an MLS library establish Kami's identity, durability, operational, or deployment correctness. Team proficiency, build reproducibility, target-platform key protection, relay admission, and actual separate-network outcomes remain unmeasured.

The smallest useful decision is therefore: approve a fixed-scope Rust feasibility trial with the same contract and network tests as the Go baseline, then choose one. Do not implement both production stacks, invent cryptography, add a broker, or select Rust merely because a future SDK might use Python.

## 2. Source basis, authority, and evidence labels

### 2.1 Local documents read

All paths below are relative to `/home/phu/dev/kami`, except the explicitly external prior-analysis directory. All existing design/planning recommendations remain drafts, not approved requirements.

| Source | Relevant constraint or finding |
|---|---|
| `AGENTS.md` | Canonical-source authority, scoped changes, review, normal implementation Jira/PR obligations. This planning task creates no Jira/GitHub objects. |
| `docs/Kami_Project_Requirements.md` §§3, 8–12 | Authenticated identities, direct/fallback, reviewed crypto, durable local state, honest delivery; full initial scope is much broader than FM. Language/transport and quantitative bounds unresolved. |
| `docs/Kami_Communication_Design.md` §§2–9, 11–13, 16–19 | Separate discovery, traversal, live relay, mailbox; transport-independent semantics; Go is an early candidate, Rust explicitly viable. |
| `docs/architecture/Communication_Architecture.md` | Conditional Go/libp2p preference, but first-message trace includes directory, tenant authorization and offline mailbox. |
| `docs/architecture/P2P_Connectivity_Design.md` | Device/Peer-ID binding, candidate handling, direct/relay lifecycle, optional AutoNAT/DCUtR, realistic network evidence. |
| `docs/architecture/Identity_And_Encryption.md` | Separate transport and application credentials, conditional HPKE/signatures, no forward-secrecy claim for static recipient keys, MLS as alternative; no approved library. |
| `docs/architecture/Relay_And_Offline_Delivery.md` | Live relay is not persistence; later mailbox needs ciphertext, authorization, transactions, retention and receipts. |
| `docs/architecture/Messaging_Protocol.md` | Later signed application envelope and portable receipts; stable IDs, dedup, state reconciliation, canonicalization unresolved. |
| `docs/architecture/Architecture_Review.md` R1–R6 | Conflicting FM scope, trust, envelopes, receipts and limits; no direct/fallback feasibility proof; QA needs FM-specific crosswalk. |
| `docs/planning/First_Communication_MVP.md` §§2–8 | Online two-CLI text milestone, durable inbox-before-ACK, direct/fallback, AC-01–AC-10; offline/groups/artifacts/adapters excluded. |
| `docs/planning/CLI_Implementation_Plan.md` §§2–12 | Go in-process engine, manual verified descriptors, live Noise, transactional outbox/inbox, one profile owner; native storage/key protection and network compatibility remain gates. |
| `docs/planning/Test_And_Validation_Plan.md` §§3–8 | Deterministic, subprocess, cross-network and adversarial evidence; unknown contracts are not testable, not passes. |
| `docs/planning/Master_Execution_Roadmap.md` §§2–9 | Reconciliation recommends online-only FM, manual verified descriptors, live/session confidentiality, stable logical identity rather than identical transport ciphertext, M0–M5/G0–G4 approval gates. |

Prior analyses read in full, from `/home/phu/.hermes/kanban/boards/kami/outputs/requirements-gap-report/`:

- `requirements-gap-report.md`: five unresolved full-release decision packages: identity lifecycle, delivery/task lifecycle, groups, adapters, operational bounds.
- `mvp-acceptance-criteria.md`: proposed full-v0.1 limits and tests, not approved FM thresholds. Its terminal-state and group-completion wording must not override the later review's honesty corrections.
- `architecture-options-tradeoffs.md`: conditional per-device/group crypto, authoritative fanout, adapter and milestone recommendations; no implementation authorization.

### 2.2 Meaning of evidence

- **Verified documentary fact (F):** observed in the local drafts or the retrieved upstream source/documentation linked below. This verifies what a source says or implements, not that Kami has built or exercised it.
- **Assumption (A):** an unverified condition required for the candidate to work.
- **Tradeoff (T):** engineering judgment about cost or risk; not a benchmark, audit result, or universal ranking.
- **Recommendation (R):** proposed action requiring owner approval.

This assessment inspected documents and upstream source only. No dependencies were installed; no build, cryptographic test, networking experiment, deployment, performance benchmark, or additional agent was run. No production-ready library combination or target OS support was established. Upstream moving branches are investigation inputs, not version pins. Some API-document extraction was incomplete or unavailable; relevant source was retrieved directly where possible. In particular, a `latest` rust-libp2p documentation URL reported a docs.rs build failure: that is a documentation-build observation, **not evidence that the crate cannot build for Kami**. No version is selected on that basis.

## 3. Non-negotiable milestone contract, independent of language

**F, local:** the master roadmap reconciles the review toward online-only FM. The older architect trace remains a broader conditional design. **R:** use the roadmap's bounded scope for comparison, subject to Product approval; do not silently merge the two traces.

| Invariant | Implementation consequence in every language |
|---|---|
| Distinct participant/device identities and authenticated endpoint keys | Verify public descriptors out of band, pin exact key/Peer-ID binding, keep import pending until verification, block unexpected key changes. A self-signature is key possession, not human or project authorization. |
| Same security on direct and relay paths | Authenticate the intended endpoint on the **inner** relayed session. Authentication only to the relay is insufficient. No plaintext/weak-suite fallback on key errors. |
| Stable logical message identity and recipient/content binding | Persist before sending; retries/path races reuse ID and logical content. Transport encryption may produce different bytes on each session. |
| Honest delivery | Sender durable outbox precedes send; atomic receiver inbox/dedup/receipt fact precedes ACK; validate receipt issuer/correlation before reporting `device_delivered`. Socket or relay success is not delivery. |
| Crash-safe dedup and uncertainty | ACK loss triggers same-ID retry and idempotent re-ACK after restart. Same ID/different content is a conflict. Timeout means unconfirmed, not proof of nonreceipt; late ACK reconciliation must be frozen before coding. |
| Finite resources and deadlines | Approve text/frame bounds separately, candidate/dial limits, retry ownership, deadlines, storage quotas and replay-safe dedup retention. No inherited full-v0.1 SLA. |
| Local security | Protected device key backend; explicit local plaintext/history/WAL/backup policy. SQLite, safe Rust, and Noise do not themselves encrypt local history. |
| Safe diagnostics | Correlate IDs, actual connection/path, durable transitions and safe reason classes; omit bodies, keys and raw addresses by default. Escape terminal control characters. |
| Real network evidence | Separate hosts/networks for direct eligibility and controlled direct-blocked fallback, separate relay, actual path inspection and fault proof. Loopback and forced-relay mode are not substitutes. |
| Scope honesty | No mailbox acceptance, background retry after process exit, tasks, adapters, groups, A2A, universal NAT reachability or production certification implied by FM. |

Advanced lifecycle/tenant services are deferred, not discarded. The manual FM trust policy must explicitly state its lack of a production revocation authority; local blocking/key-change behavior is still needed. If owners require asynchronous application ciphertext or portable signed receipts in FM, stop and re-baseline scope and dependency selection before implementation.

## 4. Concrete networking and crypto integration evidence

### 4.1 Rust/libp2p: viable composition, application work still required

**F:** the upstream [DCUtR example](https://raw.githubusercontent.com/libp2p/rust-libp2p/master/examples/dcutr/src/main.rs) composes Tokio, TCP with `noise::Config` and Yamux, QUIC, DNS, relay-client transport, Identify and DCUtR; it observes relay reservation acceptance and connection endpoints. The [relay-server example](https://raw.githubusercontent.com/libp2p/rust-libp2p/master/examples/relay-server/src/main.rs) composes the relay server behaviour, Identify, TCP/Noise/Yamux and QUIC. Rust is therefore not a speculative rewrite of all libp2p mechanisms.

**T:** `Swarm`/behaviour/event composition requires more explicit ownership and event routing than the Go host-oriented approach. Keep a continuously polled transport owner task and bounded engine commands/events. Avoid blocking it on SQL, keyring prompts, terminal input or long crypto operations. Cancellation must not discard a committed inbox or make a timed-out caller assume a send never happened. Tokio's [spawning tutorial](https://tokio.rs/tokio/tutorial/spawning) documents ownership of spawned tasks; borrowing and task boundaries must be designed, not translated mechanically from Go contexts.

**F:** the inspected [Rust relay behaviour](https://raw.githubusercontent.com/libp2p/rust-libp2p/master/protocols/relay/src/behaviour.rs) exposes reservation/circuit counts, duration/byte caps and rate limiters. Its default `max_circuit_bytes` is documented as **128 kibibytes**, while the CLI draft proposes a **256 KiB whole frame**. These are different layers, but a maximum-size application frame cannot be assumed to traverse a lower-byte-budget circuit, even before secure-channel overhead and receipts. **R:** explicitly configure and test circuit budgets against the approved encoded-frame plus handshake/receipt allowance; reject oversize locally or negotiate permitted limits, never raise quotas silently or infer library defaults satisfy Kami bounds.

**F:** that Rust relay `Config` does not expose the same application ACL interface as Go's relay source below. **A:** an approved Rust admission mechanism can enforce both source and destination restrictions using the pinned library's connection/behaviour controls or a reviewed wrapper. **R:** prove rejection before useful forwarding; a generic connection allowlist alone must not be described as project authorization or a source/destination ACL equivalent. No library fork is preauthorized.

Upstream examples use deterministic seeded identity generation, broad address logging and demonstration defaults. **R:** do not copy those into Kami key generation or operational logging. Generate identities with a library-backed secure RNG, use protected storage, and redact underlying library logs as well as engine logs.

### 4.2 Go/libp2p: credible baseline, not an already-validated solution

**F:** [go-libp2p](https://github.com/libp2p/go-libp2p) is an existing implementation, and its [Circuit Relay v2 source](https://raw.githubusercontent.com/libp2p/go-libp2p/master/p2p/protocol/circuitv2/relay/relay.go) checks `AllowReserve`, `AllowConnect`, destination reservations, circuit limits and resource scopes. This is a concrete integration advantage for a restricted relay, not a claim that a complete Kami authorization service exists.

**T:** Go's goroutines, channels and cancellation contexts are a comparatively direct fit for the existing proposed engine interfaces. Application code must still bound goroutines, connect/ACK waits, stream allocations and queues; recover transaction results after cancellation; stop workers cleanly; and prevent data races. A mature network component does not supply durable message IDs, inbox-before-ACK or a correct delivery reducer.

**R:** use the same peer-binding, direct/fallback, resource-limit and crash test contract for Go and Rust. Do not credit Go for network behavior merely because the older plans named it; do not penalize Rust for missing presumed conveniences without checking the pinned API.

### 4.3 C and C++: native access is real; libp2p parity is not established

**F:** [cpp-libp2p's README](https://github.com/libp2p/cpp-libp2p) lists TCP, SECIO/plaintext, Yamux/MPlex and Identify. It does not establish the required Noise + Circuit Relay v2 + DCUtR combination. Its C++ standard/build-manager descriptions are also inconsistent across repository surfaces. **Important limit:** documentation omission is **not proof those features are absent from current source**. No pinned C++ feature audit or build was performed here. Treat claimed parity as unverified; do not adopt plaintext or SECIO as a shortcut to make the comparison pass. No equivalent C libp2p combination was verified in this assessment.

A concrete native alternative exists: [libdatachannel](https://github.com/paullouisageneau/libdatachannel) implements WebRTC data channels in C++ with C bindings; it documents SCTP/DTLS, ICE/STUN/TURN, Linux/macOS/Windows and other platforms, selectable TLS and ICE backends, and application-provided signaling. It depends on native components including a TLS provider and usrsctp; optional media can be disabled. **T:** this is a coherent C/C++ candidate, but a different architecture: authenticated SDP/certificate-fingerprint binding, ICE candidate exchange, TURN credentials/operations and retry/candidate policy must replace the libp2p assumptions. C uses its C ABI but does not thereby eliminate the C++ dependency/build graph.

Go also has a concrete [Pion WebRTC](https://github.com/pion/webrtc) alternative, documented as pure Go with DataChannels, ICE, STUN and TURN. Rust can use native bindings such as those linked by libdatachannel, but that imports native dependency and unsafe-boundary obligations; it is not evidence of a simpler Rust FM stack. **R:** evaluate one WebRTC alternative only if the libp2p feasibility gate fails a required profile, or browser compatibility becomes an approved near-term constraint. Do not deploy libp2p plus TURN plus a second signaling stack preemptively.

### 4.4 Crypto availability must be evaluated by layer

**F:** the [noise-libp2p specification](https://raw.githubusercontent.com/libp2p/specs/master/noise/README.md) defines a channel-security handshake, including authentication of Noise static keys using libp2p identity keys. **R:** integrate the network library's implementation; do not build a second standalone Noise handshake. Kami must bind the resulting authenticated Peer ID to the intended verified device and enforce application authorization.

For online FM, an endpoint-authenticated secure stream through a ciphertext-forwarding relay is the candidate live confidentiality boundary. It is **not** an asynchronously decryptable mailbox format or a portable device signature. If QUIC is enabled, its own TLS-secured transport is a separate security configuration to review; do not call a QUIC connection Noise-over-TCP or assume Yamux is layered over QUIC.

For later asynchronous E2EE, the architect proposes HPKE per-device encryption plus signatures or a mature ratcheting construction. No such suite/library integration is approved. HPKE plus a raw signing primitive is not a complete reviewed messaging protocol: canonical signed bytes, key binding, replay/expiry handling and compromise guarantees require separate review. The inspected [Cisco Go HPKE repository](https://github.com/cisco/go-hpke) is archived and describes a draft implementation; it is not a maintained RFC 9180 dependency recommendation. This **does not establish that Go lacks current HPKE implementations**. A Rust HPKE candidate lookup was unsuccessful, so this assessment makes no verified Rust-HPKE-library maturity claim.

For later groups, [OpenMLS](https://github.com/openmls/openmls) documents an RFC 9420 implementation in Rust, pluggable crypto providers, specific built/tested targets, storage options, and sensitive content/crypto debug features. **T:** native integration is a genuine Rust advantage if MLS is actually selected, but it does not select the provider, prove audit coverage of the exact version, solve membership/epoch recovery, or make MLS necessary for FM. Disable sensitive debug features in distributable configurations and verify the effective feature graph.

[Cisco MLS++](https://github.com/cisco/mlspp) offers a C++ path with OpenSSL/BoringSSL and CMake-related dependencies. Go could integrate native crypto through an ABI or service boundary rather than requiring every protocol to be reimplemented in Go. Those choices add maintenance and packaging costs, not impossibility. The [libsignal README](https://raw.githubusercontent.com/signalapp/libsignal/main/README.md) explicitly says outside-Signal use is unsupported and APIs/bridges can change, and lists AGPLv3 licensing and substantial native build tools. Its Rust implementation is therefore not automatically a supported drop-in Kami SDK in any language.

**R:** do not choose Rust solely to preselect MLS, or C++ solely for OpenSSL access. Freeze the approved threat model and protocol first; then check maintenance, license, security-review coverage, exact suite/provider, supported targets, test vectors and persistent key-state recovery. No fetched source provides a security certification for Kami's eventual integration.

## 5. Implementation comparison

The table combines the documentary facts above with **T: engineering judgments**. Complexity is relative to this FM scope, not a popularity score or measured delivery estimate. Maintainer expertise could reverse the relative implementation costs.

| Dimension | Rust | Go | C / C++ |
|---|---|---|---|
| Implementation complexity | Explicit ownership, enums and fallible APIs suit protocol state; async lifetimes/traits and Swarm event composition add initial design cost. | Closest to existing proposed interfaces; goroutine/context composition usually reduces initial orchestration work. | C requires explicit cleanup and error discipline; C++ RAII helps but callback ownership, templates, ABI and exception policy still add integration work. |
| Dependency maturity for FM | Concrete libp2p direct/relay/DCUtR examples and server behaviour; exact-version feature/platform evidence still required. | Concrete libp2p relay ACL/resource mechanisms and existing proposed module mapping; still no Kami integration proof. | Mature individual native networking/crypto components are available, but inspected C++ libp2p documents do not prove FM feature parity; WebRTC alternative changes architecture. |
| Async runtime | Tokio plus futures/libp2p stream/event ownership; isolate synchronous storage/keyring work and test cancellation. | Built-in goroutines/netpoll model and contexts; contexts do not automatically cancel arbitrary driver/FFI work. | Choose one event-loop/callback model for chosen library; bridge foreign threads carefully. Do not mix several runtimes without a reason. |
| Cross-platform packaging | Cargo lockfile/toolchain plus target-native linkers; SQLite/SQLCipher or crypto dependencies can require C/C++ tools. No automatic pure-Rust/static binary claim. | Straightforward packaging if dependency graph is cgo-free; cgo reintroduces target C toolchain/linkage. Keyring/platform APIs may still vary. | Per-target compiler, standard library, TLS/SCTP/ICE linkage, runtime/ABI and redistribution validation; C API does not remove underlying C++ runtime. |
| Testing | Proposed `cargo test`, fmt/Clippy, property/parser fuzzing, subprocess/crash tests; Miri for supported unsafe-boundary tests, not a universal networking verifier. | Proposed unit/golden tests, race detector, vet, fuzzing, subprocess/crash tests. Race detector does not prove absence of races or inspect arbitrary native code. | Proposed CTest/library test framework, compiler warnings/static analysis, ASan/UBSan/TSan in supported configurations, fuzzing; substantial memory/concurrency coverage burden. |
| CLI / daemon architecture | Small command-mode executable hosting core engine; one runtime and one profile owner. Rust does not require a daemon. | Same in-process architecture; no daemon required by goroutines. | Same possible; native callback service lifetime must be owned independently of CLI parsing. No daemon requirement intrinsic to C/C++. |
| Native library integration | C ABI via reviewed bindings, unsafe encapsulated; own allocation/lifetime/callback-thread/unwind contract. C++ often needs a C shim. | cgo and C shims; pointer-pinning/callback/lifetime restrictions and cross-compilation requirements documented in [cgo](https://pkg.go.dev/cmd/cgo). | C libraries integrate directly; C++ source integration easier within compatible toolchain, but unstable C++ ABIs still matter across shared-library boundaries. |
| Memory safety | Safe Rust prevents broad classes of lifetime/data-race bugs; unsafe/FFI/native deps remain in threat boundary. Does not prevent authorization, replay, deadlock or crypto mistakes. | Managed memory reduces manual lifetime bugs; races, unsafe/cgo and resource leaks still require controls. Garbage collection is not secure erasure. | No default memory-safety guarantee. RAII/smart pointers reduce hazards but do not eliminate buffer, lifetime or data-race bugs. Least attractive default for untrusted protocol parsing. |
| Maintenance | Keep feature graph/MSRV/toolchain/native deps pinned, upgrade adapters behind stable domain APIs; Rust-specific reviewer skill needed. | Keep module/toolchain/native dependency policy pinned; avoid overcoupling core to library host types. Existing draft reduces design translation, not future maintenance to zero. | Maintain compiler/ABI/build-manager/native dependency matrix and sanitizer coverage; higher exposed parsing/lifetime review burden. |
| Future Python / agent SDK | CLI/IPC first; [PyO3](https://pyo3.rs/) and maturin provide an optional extension path, with wheel/ABI/runtime/cancellation ownership work. | CLI/IPC equally practical; native extension route generally adds C-compatible export/FFI and distribution work. | C ABI or bindings feasible, but manual ownership, exception isolation and per-platform wheels remain costs. |

**A:** the team can maintain Rust async/network code and review native boundaries. This has not been established. **R:** require the feasibility handoff to be understandable and maintainable by a named backup maintainer; if the only maintainable implementation is Go, its lower coordination risk outweighs speculative later-library advantages. No throughput, binary-size, memory-use, build-time or hiring-popularity claim is used as a deciding fact.

## 6. Candidate Rust implementation shape, if approved

This proposes logical boundaries, not new code or a frozen package layout.

- One command-mode binary and a reusable internal engine library, with a separate explicit relay-serving role/process. No TUI, daemon, broker or public discovery for FM.
- CLI → engine → identity/protocol/delivery/store/transport abstractions. Keep libp2p types and Tokio task handles out of the transport-independent protocol and delivery reducer.
- A transport owner continuously polls its Swarm and emits bounded authenticated-session/path events. A state owner coordinates messages and receipts; store requests run on a dedicated bounded worker so synchronous SQLite operations cannot stall networking.
- Candidate CLI/config/serialization components are `clap`, `serde`/`serde_json`, and redacted `tracing`, as illustrated in the upstream examples or proposed here. They are **not approved dependencies**. Select only needed features and bound input before deserialization/allocation. JSON serialization alone is not canonical signing.
- Candidate local persistence: `rusqlite` with a pinned bundled SQLite build if ordinary SQLite is approved. [Rusqlite's README](https://raw.githubusercontent.com/rusqlite/rusqlite/master/README.md) explicitly documents C SQLite bindings, bundled-source compilation via `cc`, SQLCipher feature variants and optional crypto linkage. This avoids depending on a user's arbitrary SQLite version but is **not a pure-Rust build**.
- If application-managed at-rest encryption is mandatory, separately approve/test a SQLCipher feature/provider/key-delivery combination before real private data is stored. Stock bundled SQLite does not satisfy that requirement. An explicitly approved encrypted-volume lab profile is a narrower alternative, with plaintext DB/WAL semantics disclosed.
- Go baseline: the existing `database/sql`/`modernc.org/sqlite` candidate avoids the ordinary cgo driver path, but the [retrieved package page](https://pkg.go.dev/modernc.org/sqlite) supplies dependency metadata, not an encrypted-store guarantee. Compare an actually pinned driver and native graph, not language labels.
- Private key provider stays abstract until Linux headless and any claimed macOS/Windows backend is proven. Unavailable/locked provider fails closed; never regenerate an identity or write plaintext keys as fallback. No specific Rust keyring crate/backend was validated here.
- One live process owns a profile, DB and retry engine. A second `send` against a live listener must fail clearly or wait for a separately approved IPC model. Exited CLI processes retain pending outbox but perform no background retries.

For later Python adapters, prefer a versioned JSON CLI/subprocess boundary first; a persistent authenticated local API can follow when daemon semantics are approved. Specify local caller authentication/permissions, bounded streams, request IDs, cancellation and event replay before adding sockets. Python clients should not reimplement P2P, key storage or the delivery reducer. A PyO3 module may later wrap the stable engine, but must not create competing Tokio loops/profile owners or equate Python cancellation with non-delivery. Harness adapters still perform recipient-side authorization and never execute received chat text.

## 7. Separate-network direct P2P and fallback assessment

**A:** both clients can open outbound connections to a separately operated self-hosted relay, and at least one approved network profile permits a direct path without manually exposing public inbound ports on both hosts. No language makes this assumption true by itself.

**R path policy:** after trust and durable enqueue, try approved direct candidates within a finite budget while preserving a viable relay reservation/path; on direct failure, use the authenticated circuit and inner endpoint session. Any DCUtR upgrade or connection race preserves logical ID, content binding and recipient dedup. Use actual connection ID/endpoint metadata for each attempt: an endpoint can have direct and relay connections simultaneously, so a generic peer-level “connected” flag cannot prove a particular message took the direct path.

| Environment / fault | Required evidence and honest outcome |
|---|---|
| Two recorded networks, direct eligible | Authenticated direct connection carrying the Kami frame and device ACK, not merely a successful ping, LAN discovery or relay-assisted connection. Record why direct is eligible and whether hole punching was needed. |
| Both endpoints NATed / CGNAT | Try supported rendezvous/Identify/DCUtR candidates as approved; report actual outcome. Some profiles will require relay. No universal punching or direct success claim. |
| UDP blocked, relay TCP reachable | If the pinned combination supports it, live circuit carries inner endpoint-authenticated stream. Failure of QUIC does not authorize plaintext or an HTTP-proxy-bypass claim. |
| Direct prevented, relay reachable | Prove injected direct failure, `auto` fallback, actual circuit path, stable ID, one inbox/display and valid receiver ACK within approved budget. Forced-relay mode alone fails the evidence gate. |
| Direct dies after receiver commit / ACK lost | Same-ID retry over relay, durable duplicate suppression and re-ACK after restart; no false nondelivery or new visible message. |
| Reservation expired / relay quota denied | Explicit retryable/permanent reason per approved contract, bounded reservation refresh, retained outbox; no mailbox or delivered status. |
| Relay down, both paths blocked, recipient offline | Visible unconfirmed/no-route outcome with retained outbox and inactive-retry disclosure after exit. FM cannot promise remote storage or eventual offline delivery. |
| HTTPS-only authenticated enterprise proxy | Unsupported until an explicitly tested/configured transport exists. Raw TCP on port 443 is not automatically HTTPS/proxy-compatible; do not add WebSocket/TURN without a separate decision. |

Relay operation still needs a public endpoint, infrastructure keys, admission/rate/byte/connection limits, reservation renewal, health, capacity and upgrade/incident ownership. Both implementations must prove source/destination denial, not merely expose a reservation API. Inspect library-level debug logs for addresses and keys as well as application diagnostics. A sentinel scan supports only the declared inspected boundary; endpoint authentication and reviewed configuration are separate security evidence.

## 8. Smallest bounded proof-of-concept programme — proposed, not executed

**R:** authorize only the following sequential experiments after G0 scope approval, with a checkpoint after each. Proposed budget: **five engineer-days total for the initial Rust trial**, excluding provision of externally controlled hosts and owner review. This is a stop/review budget, not a delivery estimate or SLA. Do not run a parallel full Go build; use a bounded Go comparison only if an identified Rust blocker might be Rust-specific. Budget extension or a WebRTC pivot needs owner approval.

| Experiment | Minimal artifact / procedure | Pass / stop decision |
|---|---|---|
| POC-1 pinned composition, identity and packaging | Disposable two profiles and third relay; one bounded frame/ACK over direct and circuit streams; pin exact compiler/libp2p features; bind verified device to Peer ID; wrong/changed key negatives; generate secure keys, not upstream seeded examples. Build and execute on Linux and each claimed portability target. Inventory link/native deps and licenses; prove relay source/destination admission and byte-budget compatibility. | Pass only if intended endpoint authenticates on both paths, unauthorized forwarding fails, limits fit agreed frames, builds run and maintainer can explain integration. Stop on required fork, unreviewed crypto, missing admission or unsupported mandatory target. Return exact blocker, not a rewritten protocol. |
| POC-2 durable protected state under async load | Add only outbox/inbox/dedup and stored receipt fact, one profile lock and chosen key/history backend. Crash sender after enqueue; receiver after commit before ACK; retry/restart; inject store-full, key-provider-unavailable and same-ID/different-content. Keep Swarm responsive during store/key operations. | No send before durable enqueue, no ACK before durable inbox, one durable display event, explicit unconfirmed and late-ACK facts, no plaintext-key fallback, no unbounded worker queue. Stop if protected storage or crash contract cannot be met without scope change. |
| POC-3 actual two-network reachability and fallback | Reuse same tiny implementation on two independent networks plus third relay. Capture direct-eligible success; direct-blocked auto fallback; UDP-blocked profile if supported; relay outage/reservation expiry and lost ACK/path-race. Inspect actual frame connection/path and endpoint authentication. | Both direct and genuine auto fallback demonstrated on approved profiles, stable logical ID and one inbox item. Publish raw timing/fault evidence against owner-approved bounds; no loopback/forced-relay substitution. If only relay works, FM gate fails; decide Go comparison versus WebRTC redesign according to root cause. |

All POCs must record exact dependency/toolchain/features, build command/result, OS/architecture/linkage, profile/trust setup, safe IDs, actual transport, fault schedule, durable transition order, selected limits and excluded cases. Redact addresses and content in retained reports. Predeclare sample counts and deadline/size criteria with QA rather than importing the prior 95/100, p95 or 24-hour proposals. Single successful POC messages establish feasibility only; they do not pass the complete FM acceptance suite.

An OpenMLS/HPKE/ratchet spike is **not on FM's critical path** under the online scope. Before later C3/C4 approval, run one small separately authorized crypto/storage-state experiment with exact suite/provider, published test vectors, persistent state/restart, key changes, offline epoch/rejoin and license/security-review coverage. If portable application ciphertext is required in FM instead, move that experiment before POC-1 and reapprove budget/scope; do not bolt custom envelope crypto onto the live prototype.

### Decision rule after the bounded trial

1. **Select Rust conditionally** only if POC-1–3 meet the approved contract/network/platform gates, protected storage is operable, no unsupported crypto/native workaround is required, and named maintainers can own the async implementation. Then proceed to full FM implementation and independent QA; POC completion is not release approval.
2. **Prefer Go** if both pass and Rust has no concrete approved native/crypto integration benefit or maintainer advantage sufficient to justify additional ownership/build complexity. Compare actual findings, not assumed rewrite velocity. Go remains viable for an online-only milestone.
3. **Investigate Go on the same minimal fixture** if Rust fails due to a library API/build/admission gap plausibly absent in Go. Do not repeat the network test expecting a language change to fix a relay that cannot be reached by either client.
4. **Return to network/security owners** if both fail a required egress, trust, storage or reviewed-crypto constraint. A bounded Pion or libdatachannel ICE/TURN investigation is then a separate approved option, with authenticated signaling, relay deployment and the same delivery tests.
5. **Choose C/C++ only with positive evidence** of an indispensable native integration or maintainer constraint plus a passing equivalent matrix. Native library availability alone is insufficient. Otherwise keep native code behind narrow reviewed interfaces rather than selecting it for all untrusted message parsing.

## 9. Documents requiring revision if Rust is selected

No existing document is changed by this assessment. Owners must reconcile these edits as an explicit later planning change set; a Go-to-Rust text replacement alone would leave security and milestone conflicts unresolved.

| Document | Smallest necessary later revision |
|---|---|
| `docs/Kami_Communication_Design.md` §16 and §§17–19 | Update candidate language/network rows and recorded decision evidence, not requirements; distinguish TCP/Noise/Yamux from QUIC security and FM from later asynchronous crypto. |
| `docs/architecture/Communication_Architecture.md` | Replace conditional Go preference with proven pinned Rust combination; mark directory/mailbox/group trace as later conditional scope, preserving full-release obligations. |
| `docs/architecture/P2P_Connectivity_Design.md` | Rust Swarm/behaviour integration, exact transports, reservation/admission controls and budgets, actual path evidence, DCUtR/Identify/address disclosure policy; retain correct key binding and network limitations. |
| `docs/architecture/Identity_And_Encryption.md` | Approved FM live-session identity/trust and protected-key backend; separate later application-envelope credentials/suite/library/provider. Rust does not by itself approve HPKE, MLS or recovery. |
| `docs/architecture/Relay_And_Offline_Delivery.md` | Rust relay quota/admission/renewal/operator details; retain distinct later transactional mailbox and authenticated portable-receipt contract. |
| `docs/architecture/Messaging_Protocol.md` | One FM frame/receipt/version/state table, live authentication versus later signatures, no identical transport-ciphertext assertion; approve late ACK/replay/content-conflict rules independent of language. |
| `docs/planning/CLI_Implementation_Plan.md` | Replace Go package/dependency/command assumptions with actual Cargo/module boundaries, Tokio/Swarm ownership, storage worker/native builds, Rust checks and platform key backend; preserve CLI/JSON/exit, profile lock, durability and domain interfaces unless separately approved. |
| `docs/planning/First_Communication_MVP.md` | Clarify J2's same logical ID/content versus different session ciphertext and exact live E2EE boundary; reference selected supported profile without broadening scope or weakening AC-01–AC-10. |
| `docs/planning/Test_And_Validation_Plan.md` | FM-only AC crosswalk, pinned feature/build matrix, Rust cancellation/store-worker/relay cap/admission negatives, native-boundary checks; keep real E3/E4 and later full-release suites separate. |
| `docs/planning/Master_Execution_Roadmap.md` | Replace Go preference with evidence-backed decision, insert bounded feasibility gate before implementation, map Rust ownership/packaging dependencies and stop conditions to M1–M5/G1–G4. |
| `docs/architecture/Architecture_Review.md` | Preserve prior findings as historical review evidence; request an addendum or superseding review of reconciliation and actual evidence, not deletion/relabeling of R1–R6. |

`docs/Kami_Project_Requirements.md` needs **no language-driven requirement change**: it is language-agnostic and its full initial scope must remain intact. Any product-scope amendment is a separate owner decision. `AGENTS.md` and the prior gap analyses need no implementation-language edit. Update this assessment's decision status through the normal review/approval process if evidence later changes; do not retroactively describe today's planning findings as experiments performed.

## 10. Owner decisions and handoff

Before implementation, Product approves online FM versus portable application ciphertext; security/architecture approves verified-descriptor binding, key-change/block policy, reviewed live secure-channel configuration, protected key/history backend and exact wire/receipt semantics; networking/operations approves network/OS profiles, relay admission/capacity, candidate disclosure and finite budgets; QA approves scope-specific samples, fault controls and the AC crosswalk. Maintainers confirm Rust competence and native dependency ownership. None of these is settled by a language preference.

Independent review should challenge especially: (1) whether Rust relay admission can meet the intended source/destination policy without unsupported modification; (2) whether frame and circuit budgets are compatible including overhead; (3) whether synchronous storage/key operations can preserve network liveness and crash-safe receipts; (4) whether direct success and fallback are genuinely distinct separate-network observations; and (5) whether a claimed future crypto advantage actually uses an approved protocol/provider with supported integration.

**Final recommendation:** Rust is a practical candidate, Go remains a strong baseline, and C/C++ is not the preferred whole-engine implementation on available evidence. Authorize the bounded feasibility investigation, not an unconditional Rust migration. Do not relax identity, durable delivery, persistence, diagnostics or validation requirements to make any candidate appear feasible.
