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
- `lib`: exposes the transport library boundary consumed by `kami-runtime`.

The crate depends on `kami-identity` and libp2p 0.57.1 with only Tokio, QUIC,
TCP, Noise, and Yamux features. `kami-identity` supplies sealed credentials;
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

No payload/stream protocol, ACK, receipt, Delivered state, production E2EE,
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