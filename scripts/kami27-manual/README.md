# KAMI-27 owner-operated native endpoint test (Linux)

This directory packages the already-existing ignored `kami-runtime` manual endpoint harness. It does not implement authentication, perform a trust decision, or send data. Successful fingerprint-confirmed tests remain owner-operated and **pending** until the owner completes them and records the result.

## Scope and safety

- Linux only: the harness opens its own D-Bus session, uses a temporary isolated `HOME`, and restricts addresses to loopback. It does not use your normal Kami profile, private keys, or desktop keyring.
- No package installation is performed. Required commands are `cargo`, `git`, `dbus-run-session`, and `gnome-keyring-daemon`; the latter is checked for presence only. The harness uses an isolated temporary HOME and does not access your personal keyring.
- The only test command used is:

  ```bash
  cargo test --locked -p kami-runtime transport_tests::native::manual_endpoint -- --exact --ignored --nocapture
  ```

- Nothing here types into another process. The owner types only into the visible test prompts. Never paste private key material, passwords, or secrets.
- QUIC/TCP here exercise the loopback native endpoint harness. This is not cross-network, relay, production, cross-platform, messaging, or E2EE qualification.

## Complete owner workflow

Use two terminals on the same Linux machine. Each endpoint is a separate invocation and gets a fresh temporary identity; do not share a terminal or an invitation between different runs.

1. In either terminal, run `./scripts/kami27-manual/setup.sh` from the repository root (or invoke it by absolute path). This checks prerequisites and repository/harness symbols; it does not start a test or alter configuration.
2. In terminal A run `./scripts/kami27-manual/start-a.sh`. In terminal B run `./scripts/kami27-manual/start-b.sh`. Each starts the exact ignored test command above. Leave both running. Wait for each to print `LOCAL_PEER=...`, `LOCAL_FINGERPRINT=...`, and `INVITATION=...` plus its `LISTEN` addresses. Treat these outputs as public test identity data; do not use an invitation's displayed data as fingerprint verification.
3. Exchange each endpoint's `INVITATION` hex by copy/paste through the harness prompts. Do not send the invitations to an untrusted channel.
4. Each person reads their own endpoint's full `LOCAL_FINGERPRINT`. Compare A's value with B and B's with A through an independent trusted channel (for example in person or a previously authenticated call). Compare the complete values exactly; do not compare values copied from an invitation or rely on the test terminal alone.
5. At each `Enter full remote fingerprint ... (or REJECT)` prompt, type the full independently received remote value only if it matches; otherwise type `REJECT`. At the next prompt, type `VERIFY` only after that independent full comparison succeeded. Otherwise type `REJECT`. This is the owner's trust decision; no helper automates it. A rejection should end that endpoint with `operator rejected; no trust persisted` (or the mismatch/rejection diagnostic) and must not be treated as a successful connection test.
6. Wait until **both** terminals print `LOCAL_VERIFIED_DURABLE: wait until BOTH terminals reach this line before dialing`. Do not dial before that point.
7. Using the prompts in the live terminal (not the helper), run this sequence and wait for the peer to remain running throughout:
   - Type `quic`; paste the other endpoint's QUIC `LISTEN` multiaddress when requested. Expect `AUTHENTICATED_REMOTE_PEER=<peer-id> TRANSPORT=quic`.
   - Type `disconnect`; expect `DISCONNECTED: old capability revoked`.
   - Type `tcp`; paste the other endpoint's TCP `LISTEN` multiaddress. Expect `AUTHENTICATED_REMOTE_PEER=<peer-id> TRANSPORT=tcp`.
   - Type `disconnect` again.
   - Reconnect by typing `quic`, paste the same peer QUIC listen address, and expect `AUTHENTICATED_REMOTE_PEER=<peer-id> TRANSPORT=quic`; then disconnect. This checks reconnect after revocation. The peer must stay running; each active connection is disconnected before selecting another transport.
8. To finish, in each terminal type `quit` at its command prompt, or press Ctrl-C in that same terminal. Then run `./scripts/kami27-manual/cleanup.sh` if desired for the reminder. It deliberately deletes or signals nothing; the harness owns temporary-directory cleanup.

### Automatic actions vs owner actions

**Automatic:** `setup.sh` checks dependencies/platform/source symbols. `start-a.sh` and `start-b.sh` launch only the exact existing ignored harness command. The test harness creates temporary test state and its private D-Bus session.

**Owner only:** invitation exchange; full fingerprint comparison through an independent trusted channel; typing the remote fingerprint and deciding `VERIFY` or `REJECT`; entering QUIC/TCP/disconnect commands and addresses; and ending each harness process. The `authenticate.sh`, `connect.sh`, `disconnect.sh`, `negative-tests.sh`, and `cleanup.sh` scripts print instructions only. They never inject input or choose trust.

## Expected output and result criteria

Expected startup lines include `LOCAL_PEER=...`, `LOCAL_FINGERPRINT=...`, `INVITATION=...`, transport `LISTEN` addresses, and then `LOCAL_VERIFIED_DURABLE` on each verified endpoint. A successful dial prints `AUTHENTICATED_REMOTE_PEER=<peer-id> TRANSPORT=quic` or the corresponding `TRANSPORT=tcp`; a deliberate teardown prints `DISCONNECTED: old capability revoked`. Final success also requires each harness invocation to exit successfully (cargo/test process exit status 0) without an intervening panic or error. An address appearing by itself is not proof of an authenticated connection.

Record **PASS** only if both independently compared identities were explicitly verified by their owners, both endpoints reached `LOCAL_VERIFIED_DURABLE`, QUIC and TCP connections reported success, disconnect/reconnect sequence completed, and both harness invocations exited 0. Record **FAIL** for an unexpected fingerprint, failed/missing trust confirmation, failed connection or disconnect, crash/panic, nonzero exit, or any ambiguity. Stop and investigate; do not repeat verification by silently trusting an identity. A `REJECT`/wrong fingerprint run demonstrates rejection behavior only, not successful endpoint testing. Worker packaging/automated fixture results are not an owner PASS.

## Negative-test limits and automated fixture

A two-endpoint wrong-Peer socket negative cannot be claimed from this manual harness: with only two identities, the operator can simply verify the actual remote peer. The harness supports owner rejection at the fingerprint prompt, but that is not a wrong-Peer socket test. The existing strict automated three-identity negative fixture is in `crates/kami-transport/tests/direct.rs`; this helper does not run it. Do not infer fixture results from these scripts.

## Troubleshooting

- `required command not found`: install/enable prerequisites through your normal system process; scripts never install packages. Ensure `cargo`, `git`, `dbus-run-session`, and `gnome-keyring-daemon` are available on PATH; the last is only checked for presence and the personal keyring is not accessed.
- `harness is Linux-only`: this owner-operated flow is not supported on macOS or Windows.
- `cargo` cannot resolve/build the package or the locked test fails: preserve the complete diagnostic and exit code; do not edit the harness or lockfile as part of this procedure.
- No `LOCAL_VERIFIED_DURABLE`: both owners must finish their local comparison and explicit confirmation. Do not dial early. A fingerprint mismatch/rejection must not be overridden.
- `only selected loopback transport is permitted` or invalid address: use the matching `LISTEN` address printed by the other live endpoint: QUIC includes `/udp/` and `/quic-v1`; TCP includes `/tcp/`.
- `disconnect before selecting another transport`: disconnect the active capability before trying another transport.
- Connection timeout or error: ensure the other endpoint is still running, use the correct transport's listen address, and report the exact diagnostic. Do not treat retrying with a different identity as a fix.
- To stop, type `quit` in the harness prompt or Ctrl-C in that exact terminal. The cleanup helper does not terminate processes or remove files.

## Qualification status

This package and its shell syntax/path checks do not establish that an owner completed the workflow. Successful owner fingerprint-confirmed QUIC, TCP, and reconnect tests are **manual-success-pending**. This procedure is Linux-only and loopback-only; Windows/macOS, cross-network behavior, relays, production readiness, and application-level message/E2EE behavior remain unqualified by it.
