# kami-transport

Authenticated, bounded direct-connection substrate for KAMI-27. This crate
validates established libp2p connections against the expected identity and
issues revocable capabilities for runtime supervision. It does not implement
application messaging or claim production delivery.

## Modules and interfaces

- `policy`: bounds candidate addresses and dial attempts and defines the direct
  connection policy.
- `connection`: validates the authenticated remote PeerId and performs the
  fresh identity/trust decision before authorizing a connection.
- `driver`: owns the libp2p swarm, bounded/cancellable dialing, connection
  events, and capability lifetime/revocation.
- `prekey_exchange`: fixed protocol framing, request-response behaviour and
  connection-bound evidence for prekeys only, not arbitrary bytes.
- `lib`: exposes the transport library boundary consumed by `kami-runtime`.

The crate depends on `kami-identity`, transport-independent `kami-protocol`, and
libp2p 0.57.1 with Tokio, QUIC, TCP, Noise, Yamux and request-response features.
`kami-identity` supplies sealed credentials;
transport does not access identity seed material. `kami-runtime` owns production
startup and supervision; transport is not coupled to storage or the message,
sender, receiver, or E2EE crates.

## Behavior and limits

The direct path uses exact IP socket candidates, a cap of eight candidates per
attempt, bounded attempts/connections and timeouts, cancellation, expected-peer
dial binding, and actual `ConnectionEstablished` peer validation. Capabilities
are revocable and checked against live state, generation, and expiry. The runtime
must retain the profile lease and revalidate enrollment/custody before use; this
is a runtime invariant rather than compile-time proof.

`prekey_exchange` adds only `/kami/prekey-exchange/1` on the same swarm:
strict protocol-owned query/selection/publication-ACK/result/typed-error frames.
The four-byte length prefix is included in the 16-KiB cap; payload allocation
is bounded before reading. Admission permits one exchange per peer/direction,
16 globally, with a five-second deadline further bounded by capability expiry.
Driver evidence binds exact typed bytes to authenticated peer, connection ID and
revocable capability; disconnect, dropped waiter, expiry and shutdown fence it.
Runtime uses a separate reliable inbound queue of 16, not diagnostic events;
driver command/event queues are bounded at 64/32. A response carries exactly one
independently signed public selection, not an aggregate signature or batch.
Publication ACKs are not application delivery receipts.

Runtime/SQLCipher freeze exact signed response plus global OTK reservation before
send and retain verified selection before publication ACK. Matching exact-byte
retry and duplicate ACK/result reconcile; conflicts reject. Transport itself
does not sign, choose OTK/fallback policy, persist or globally mark provider keys
published. Identity/native custody and durable generation/validity checks remain
runtime responsibilities, not authority derived from a parsed frame.

WP-B is bounded accepted with conditions: implementation `t_d364301b` run 204,
independent QA `t_0dcc13ac` run 205 PASS; final Evaluator `t_250e6416` run 206
PASS-WITH-CONDITIONS. Later WP-C is owner-accepted only as a bounded recovery
foundation with conditions/limitations: implementation `t_e3a1ba25` run 229,
QA `t_e022f9b4` run 230 PASS, Evaluator `t_10759b1a` run 231
PASS-WITH-CONDITIONS. Storage defaults to process-local Disabled on every startup;
only native one-use FreshEnrollment bound to the actual encrypted DB through the
worker-local handoff admits authority. Restart/quarantine permits exact-artifact
reconciliation, not new cryptographic transitions. A new authenticated transport
connection cannot substitute for fresh enrollment. WP-C refuses unsafe restoration,
not rollback detection; no independent witness, transparent mutable-session
restoration or live fresh-session establishment is supplied. No transport source
changed for WP-C. Reused QA workspace evidence: 284 passed, 0 failed, 3 existing
ignored; Linux private-D-Bus/native only, no Cargo rerun here. WP-D network/platform/
interoperability/fuzz/DoS/load and owner-operated WP-C qualification remain open;
production E2EE/application messaging, KAMI-28, full KAMI-51 and M1 remain incomplete.
The [WP-B runner](../../scripts/kami51-wpb-manual/README.md) now includes an ignored
two-terminal prekey/status/exact-retry interface as well as real automated fixtures.
Neither interface availability nor automated review establishes owner WP-B manual
qualification or WP-C restart qualification.

No generic payload API, application receipt/Delivered state, production E2EE,
session restoration, relay, NAT traversal/DCUtR, IPC, or CLI activation is
implemented. Loopback handshake tests on Linux do not establish cross-network,
Windows/macOS, or production qualification.

## Focused verification

From the repository root:

```sh
cargo test --locked -p kami-transport --test direct real_native_quic_tcp_and_fallback -- --exact --nocapture
cargo test --locked -p kami-transport --doc -- --nocapture
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
```

The focused native QUIC/TCP/fallback loopback test and compile-fail doctests
were reported passing by QA and independently rerun by the final Evaluator on
Linux. Doctest compiler errors are expected assertions, not failures.