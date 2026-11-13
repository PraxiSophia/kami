# KAMI-51 WP-B owner-operated prekey endpoint (Linux loopback)

This launches the **ignored test endpoint**, not a production CLI or IPC API. Run
it independently in two terminals. Each process enrolls its own identity through
native Secret Service custody, owns its SQLCipher profile through `ProfileLease`,
and uses actual authenticated QUIC or TCP/Noise/Yamux. No wrapper supplies trust,
keys, peer provenance or protocol responses. **Owner manual acceptance is NOT RUN
by implementation/fixture checks.** KAMI-27 acceptance does not qualify WP-B.

## Start and independently verify

From the repository root (absolute script paths also work):

```bash
bash scripts/kami51-wpb-manual/verify.sh --help
bash scripts/kami51-wpb-manual/verify.sh --check
# Terminal A, then independently Terminal B:
bash scripts/kami51-wpb-manual/verify.sh --endpoint
```

Default `--check` runs `cargo check --locked --offline -p kami-runtime --tests`;
it does not launch a keyring, enroll identities or modify existing profiles.
The script checks Linux and `cargo`, `git`, `dbus-run-session`, and
`gnome-keyring-daemon`; nothing is installed automatically. Cargo may update build
artifacts/cache. A missing tool, cached dependency or locked/unavailable native
store is an error, not permission for plaintext/mock fallback.

`--endpoint` clears inherited fixture child markers and launches exactly:

```bash
cargo test --locked --offline -p kami-runtime transport_tests::native::manual_endpoint -- --exact --ignored --nocapture
```

The existing endpoint launcher creates a private D-Bus session, temporary HOME/XDG
and disposable keyring/profile for **each** invocation. It does not use a normal
Kami profile or desktop keyring. Do not invoke a child binary directly or set
`KAMI27_MANUAL_PRIVATE_BUS` yourself. The wrapper never injects stdin.

1. Wait for each terminal's `LOCAL_PEER`, full `LOCAL_FINGERPRINT`, `INVITATION`
   and two loopback `LISTEN` addresses. Keep both processes alive.
2. Exchange the public `INVITATION` hex through the prompts. An invitation and its
   `UNTRUSTED_REMOTE_PEER` display are **not** authenticated identity evidence.
3. Each owner reads their own full `LOCAL_FINGERPRINT` over an **independent
   trusted channel** (in person or an already authenticated call). Compare both
   complete values. Do not derive the compared value from the imported invitation.
4. Type the full independently received remote fingerprint. On mismatch, uncertainty
   or unwanted contact type `REJECT`. Type `VERIFY` at the next local prompt only
   after the comparison succeeded. Nothing automates or overrides this decision.
5. Wait until **both** terminals reach `LOCAL_VERIFIED_DURABLE` before dialing.
   Wrong/malformed fingerprints, self invitations, REJECT and missing affirmative
   verification terminate fail-closed without persisting trust.

## Owner commands and sequence

Commands are exact and bounded; no application message is accepted. The peer must
remain running. Choose one terminal as requester for the first sequence; the other
can stay at its command prompt while its engine handles incoming prekeys.

1. Type `prekey-status`: initially reports session-local status unavailable, not a
   successful exchange. `prekey` before a local dial is rejected without a network
   operation. Merely accepting an incoming connection does not select a local
   requester capability.
2. Type `quic`, then paste the peer's QUIC `LISTEN` address. Expect
   `AUTHENTICATED_REMOTE_PEER=<expected-peer> TRANSPORT=quic`. Compare it with the
   independently verified endpoint. Only selected loopback addresses are accepted.
3. Type `prekey`. On success expect `PREKEY_LAST_ATTEMPT=completed` and a
   `PREKEY_STATUS session-local last-observed` line with public fields described
   below. Record the exact `REQUEST_ID` (64 lowercase hex characters).
4. Type `prekey-status` to repeat the last observed stage without accessing storage
   or sending anything. It does not refresh trust, validity or capability authority.
5. Type `retry <REQUEST_ID>` using that exact ID. This calls
   `Engine::retry_prekeys` on the current actual capability; no new query or ID is
   silently substituted. A matching durable exchange must reconcile the same
   selection, reference and generations. A syntactically valid **unknown** ID
   should fail, not create a new query. Malformed/zero IDs are rejected locally.
6. Type `disconnect` in **both terminals**, waiting for
   `DISCONNECTED: old capability revoked`. The requester drops its capability;
   `prekey`/`retry` without another local dial are rejected. The status view remains
   explicitly historical/session-local and is never authority to use the old cap.
7. On the requester type `tcp`, paste the peer's TCP `LISTEN` address, then run
   `retry <same REQUEST_ID>` and `prekey`. The old durable exchange must reconcile
   exactly using the new authenticated capability; a fresh query has a new ID.
   A peer that retains an old inbound connection may otherwise interfere with
   transport selection, hence the explicit mutual disconnect.
8. To request in the opposite direction, have that terminal locally dial the
   verified peer and run the same commands. Each direction has a separate account
   and ledger. Do not infer publisher-side status from the other terminal's prompt.
9. Optionally disconnect both, reconnect with `quic` and retry the exact ID again.
10. Type `quit` in both terminals. Expect `CLEAN_SHUTDOWN LEASE_RELEASED`, each
    ignored test passing, and both Cargo exit codes 0. The harness awaits actual
    shutdown and reacquires/releases the profile lease before reporting release.

A failed attempt prints `PREKEY_ATTEMPT_FAILED` and
`PREKEY_LAST_ATTEMPT=failed-or-indeterminate`; it never infers success from a socket
write. If a durable request was observed, retain its ID and retry it after an
explicit reconnect. Do not replace a failed exchange with `prekey` and call that an
exact retry. If status is unavailable, no durable request ID was independently
observed by this interface; do not invent one. Parse/no-capability rejections do
not start an attempt or erase a previous observation. Unknown commands and
oversized/missing-newline input terminate the session fail-closed.

## What the public evidence actually establishes

The private `cfg(all(test, target_os = "linux"))` Engine observation seam holds
**one** last-observed requester operation. No production constructor, API,
transport/storage change or worker authority bypass is added. Its only consumer is
the ignored manual session (plus focused formatting tests). It is not a durable
ledger-query or publisher-side diagnostic API.

| Field/classification | Provenance and limit |
|---|---|
| `REQUEST_ID`, `QUERY=durable` | Existing worker `BeginPrekey` succeeded using fresh durable contact trust, exact peer/device and durable query; ID is the actual returned query ID, never generated by the shell. |
| `AUTHENTICATED_REMOTE_PEER`, `VERIFIED_REMOTE_DEVICE` | The actual live capability revalidated against freshly loaded durable trust after begin; not a caller-provided Peer-ID or cached invitation. This is authority at that observed stage only. |
| `SIGNED_KIND`, `SIGNED_REFERENCE` | Public bounded kind and hex-encoded opaque reference projected from the received signed binding proof; exposed **only after** existing `RetainPrekey` and its fresh/live fences succeed. No raw proof, provider state or secret is displayed. |
| Account/bundle/binding generations and validity interval | Actual received selection/account generation and signed binding fields, accepted by existing storage validation. References remain exact bytes, not normalized provider identifiers. |
| `FRESHNESS=valid-at-retention` | Existing signature, binding, validity, monotonic generation/revocation and request correspondence validation passed at retention. This is **not current freshness**, clock rollback resistance or an expiry monitor. Before retention: unknown. |
| `RETENTION_BEFORE_ACK=committed` | Existing worker completed durable `Retain` and returned the exact ACK before Engine sent it. Before that: unobserved, not a claim that persistence definitely did not happen. |
| `PUBLICATION_ACK_RESULT=exact-result-reconciled` | Existing worker completed `FinishPrekey`: live authenticated ACK-result validated against the exact durable retained exchange. This is requester-side reconciliation of the publisher result, **not** a new independent publisher database readback, global provider publication flag or DEVICE_DELIVERED. |
| `unobserved-or-indeterminate` | No successful completion of that stage observed. A timeout/failure may leave durable state changed; absence of observation is not absence of a commit or remote action. |

`prekey-status` does not query arbitrary IDs, persist observations across restart,
or update past observations after disconnect/revocation/expiry. A new actual
refresh/retry clears the previous observation before beginning. Failures retain
only successfully observed stages of that attempt. Do not label a past completed
observation as success for a later rejected/no-capability command. The existing
worker/transport/storage fences remain the only authority; status never enables
an operation. Mismatched/unsolicited responses, wrong peers/devices, stale/expired
capabilities, revocation and unverified trust remain fail-closed.

## Separate automated validation

```bash
bash scripts/kami51-wpb-manual/verify.sh --fixtures
cargo test --locked -p kami-runtime manual_
```

`--fixtures` runs the existing protocol, E2EE, storage, authenticated-boundary and
native runtime lifecycle tests, exiting on the first failure. Its native fixture
uses **two engines in one private child process with simulated test trust**;
it is not two independently owner-verified processes and never asserts manual
acceptance. It clears `KAMI_RUNTIME_PRIVATE_BUS` to force private D-Bus/HOME
isolation. The ignored endpoint is skipped by ordinary test runs. Safe pre-trust
quit/rejection smoke checks are not successful fingerprint verification.

Automated-only: wrong-Peer socket negatives require the existing three-identity
transport fixture; malformed wire, wrong device/request, unsolicited response,
expiry/revocation, interrupted publication, conflicting ACK, reopen, CAS races,
OTK consumption/exclusivity, fallback, fault and clock cases remain dedicated
fixtures. No operator mutation/fault/clock/revocation controls are introduced here.
No worker or script may manufacture verified invitations or automate affirmative
trust to replace the owner's two-terminal procedure.

## Safety, recording and remaining gates

The native endpoint owns daemon kill/wait and temporary directory cleanup on normal
exit. Prefer `quit`. Ctrl-C/force kill does not prove lease release, cleanup or an
interrupted-publication scenario. Do not delete lockfiles or run broad process/file
cleanup; preserve errors and identify only the exact disposable process/directory
if interrupted. No wrapper cleanup helper kills processes or deletes data.

Public peer/device/request/reference identifiers and timing are linkable metadata;
keep logs scoped to the test participants. Never enable shell tracing, dump keys,
identity seeds, provider/account pickles, raw provider state, SQLCipher keys or
sensitive database rows, or paste secrets at a prompt. The script does not capture
logs. Native memory copies, swap/core dumps and forensic erasure are not qualified.

Record the actual commands, public stage evidence and exit codes. An owner PASS
requires independently compared full fingerprints, local VERIFY on both endpoints,
expected authenticated peer/device, real QUIC and TCP selection/retention/ACK-result,
exact retry across reconnect, and clean lease release on both processes. Record
unexpected mismatch/failure/ambiguity as FAIL; rejection-only runs demonstrate
rejection, never successful exchange. **Implementation checks, independent QA and
final security evaluation remain separate from owner manual acceptance.**

This extension was scoped to implementation task `t_a328ea09`, following architecture
`t_879e6a9f` and feasibility/security `t_6c1bfeb6`. Earlier bounded WP-B implementation
`t_d364301b`, QA `t_0dcc13ac` and evaluator `t_250e6416` results do not establish this
new interface's independent QA/final security gate or owner's manual acceptance.
Subsequent independent QA `t_c21848ef` PASS and final Evaluator `t_78919fc8`
PASS-WITH-CONDITIONS qualify the bounded interface only, not an owner manual PASS.
The `scripts/kami27-manual/` files are unchanged.

Still absent: arbitrary durable historical/publisher-side status and restartable
manual profiles, transparent mutable account/session restoration and an independent
rollback witness. Later WP-C is owner-accepted only as a bounded recovery foundation
with conditions/limitations: implementation `t_e3a1ba25` run 229; independent QA
`t_e022f9b4` run 230 PASS; Evaluator `t_10759b1a` run 231 PASS-WITH-CONDITIONS.
Every worker starts with process-local default-deny mutable authority; only native
one-use `FreshEnrollment` consumed through its worker-local handoff, bound to the
actual encrypted DB and completed/readback-verified, admits that worker. Restart/
quarantine cannot restore authority from persisted rows/counters/completion records.
Exact committed artifacts and valid frozen exchanges may reconcile without
ratchet advancement; new publication from an existing persisted account remains
blocked. This refuses unsafe restoration, not coherent rollback detection. Typed
preconditions do not implement live fresh-session establishment. Reused QA evidence:
284 workspace passes, 0 failures, 3 existing ignored; Linux private-D-Bus/native
fixtures only, no Cargo rerun by this synchronization. Owner-operated WP-C endpoint
evidence remains unqualified. This temporary-profile launcher does **not** qualify
restart recovery or convert WP-B manual checks into WP-C evidence. Ledger caps
have no compaction. No KAMI-28/application messages, production E2EE, CLI/IPC,
relay/NAT/server/mailbox, cross-network, Windows/macOS, interoperability, sustained
fuzz/DoS or production/M1 acceptance is introduced. Full KAMI-51 remains incomplete.
