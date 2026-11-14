# Kami — Identity and Encryption Specification

**Status:** Owner-approved security requirements; specific cryptographic implementation requires validation  
**Scope:** Milestone 1 (M1): two verified devices, one device per identity

## 1. Trust goals and threats

Kami must prove to an endpoint that the remote participant is the **same verified device identity** it intended to contact. The network, relay, contact invitation, IP address, display name, and DNS/address hint are not trusted proofs of this identity. The relay may be curious or malicious and can observe routing metadata, restrict service, or manipulate/interrupt transport; it must not obtain plaintext merely by forwarding an encrypted message. Endpoint compromise or malware with access to unlocked keys is not prevented by transport or database encryption.

## 2. Identity model

- **Participant identity:** Stable human-facing contact identity in the M1 one-device model.
- **Device identity:** Cryptographic public/private credential for the installed endpoint; explicitly modeled even though M1 permits only one active device per participant.
- **Transport Peer ID:** Authenticated libp2p networking identity. The contact record binds the expected Peer ID to the independently verified Kami device identity, through a verifiable cryptographic association.
- **Display name:** Untrusted presentation metadata, never proof of identity.
- **Relay identity/authorization:** A separate authorization domain. Being admitted to a relay does not authenticate the end recipient.

Each accepted connection, direct or relayed, must validate the expected device identity and authenticated transport binding. Do not silently replace an expected binding with the identity offered by a new connection.

## 3. Enrollment and initial contact

1. Generate a new endpoint identity using a reviewed cryptographic implementation and an OS-backed secure key facility supported on that platform.
2. Create a contact invitation/descriptor with public identity material, expected Peer ID or authenticated binding proof, and optional connection/relay hints; include a version and expiry/validity policy where engineering deems needed.
3. Exchange invitations through an existing channel; treat the invitation as **untrusted** until its public identity is checked.
4. Compare a strong fingerprint through an **independent trusted channel**, e.g., in person or on a previously authenticated call. Require an explicit verification action before any private send.
5. Store the verified contact identity, public keys, and transport binding durably. Refuse sends to unknown or unverified identities (no TOFU).

A QR representation may be added later; it is not needed for M1. Network addresses and relay hints may become stale without invalidating the underlying verified identity.

## 4. Establishing E2EE conversations

The selected security direction is **established Signal-style authenticated key agreement and Double Ratchet**, not a bespoke crypto construction. The M1 protocol must:

- Bind initial authenticated session establishment to the independently verified identities of both endpoints.
- Establish a shared secret/session state using an established, analyzed construction with suitable authenticated key exchange and replay resistance.
- Derive fresh per-message encryption material through Double Ratchet-style progression, including appropriate skipped/out-of-order message-key handling and secure deletion of obsolete key material where supported.
- Authenticate protocol envelopes and bind each accepted plaintext message to its immutable logical message ID, origin/destination, and relevant context, so a path switch/retry cannot substitute content.
- Provide forward-secrecy and post-compromise recovery properties **only to the extent actually guaranteed by the selected and verified protocol**. Do not claim these properties merely because some key material is rotated.
- Persist and restore ratchet/session state with crash-safe semantics so retries, out-of-order arrival, and client restarts do not reuse key/nonce material or lose the ability to decrypt legitimate in-flight messages.

**Engineering validation gate:** Before shipping, identify a viable, maintained Rust implementation and a documented interoperable session-establishment profile. Confirm licensing, maintainability, protocol completeness, cross-platform support, persistence, and security review. `libsignal` should not be presumed to provide a supported general-purpose embedding API. If no defensible supported implementation exists, **escalate before implementation**; do not silently substitute stable shared-secret encryption or write an unreviewed custom protocol.

## 5. Message encryption and independent transports

Message bodies and private application fields are encrypted for the verified recipient. Relays may require nonsecret routing information, but relay access must not grant ciphertext-decryption capability. The same logical message remains authenticated when transmitted through QUIC, TCP, direct P2P, or a live relay. Transport authentication is necessary but **not a substitute** for application E2EE.

A retry must preserve the logical content and message identity. The implemented ratchet and message-state design must explicitly define how retransmissions use safely persisted ciphertext or equivalent protocol-defined replay-safe handling. Never create a new application message merely because a packet or relay attempt was retried.

## 6. Local message storage and key custody

- Store local inbox, outbox, history, receipts, and ratchet metadata in SQLite with encryption for sensitive message contents.
- Protect private identity keys and local encryption keys through **OS secure storage**: suitable Secret Service/keyring mechanisms on Linux, Keychain on macOS, and an appropriate Windows credential/key protection facility.
- OS credential stores may be locked, unavailable, or inaccessible to noninteractive/headless services. M1 fails safely and displays a clear error rather than silently writing plaintext secrets or promising unsupported pre-login service operation.
- Define key separation and safe access boundaries: a file/database copy alone should not automatically include all decrypting secrets when supported by the platform; do not overstate protection from malware under the same logged-in user.
- Verify SQLite journal/WAL, temporary files, logs, memory/core dumps, backups, and migration paths do not casually expose secrets; treat database-at-rest encryption and identity-key custody as separate controls.
- The manually started Kami background service holds needed unlocked state only for authorized runtime use; CLI and future adapters must not directly receive master/identity secrets.

## 7. Identity changes, compromise, and loss

- **Unexpected contact identity-key change:** Immediately block messages to that contact. Hold previously queued messages without redirecting them to new keys. Require independent fingerprint reverification and an explicit user action to adopt the replacement identity.
- **Routine verified session-key changes:** Must not require identity reverification when authenticated by the established protocol.
- **Loss of local identity keys:** Create a new identity and reverify with contacts. Do not promise restoration of old identity/history if required keys are lost.
- **Recovery, backup, multi-device enrollment and revocation:** Deferred beyond M1. Security-critical invalidation/compromise must still fail closed and allow users to stop messaging.
- **Deletion:** Local message/history deletion does not remotely erase previously delivered plaintext or copies retained by the recipient.

## 8. Tests and acceptance

Security qualification must include:

1. Unknown/unchecked invitation and unknown identity refuse private sends.
2. Verified binding rejects a wrong transport Peer ID, wrong device key, or unauthenticated binding.
3. Unexpected identity replacement holds all queued sends until explicit reverification.
4. Direct and relay connections provide equivalent application E2EE; relay sees no plaintext message body.
5. Replayed ciphertext, forged receipts, message-ID/content conflicts, malformed envelopes, and tampered ciphertext are rejected safely.
6. Encryption/decryption, ratchet state, duplicate detection, and skipped-message handling recover correctly across restarts, retries, and reordering.
7. Supported OS key facilities on Linux, Windows, and macOS work; unavailable or locked facilities fail closed.
8. Local database files and relevant sidecar/log paths are inspected for accidental plaintext exposure.

These tests verify a security design; they do not establish resistance to a fully compromised endpoint or anonymity from relay metadata.

## 9. Current bounded WP-C implementation contract

The requirements above remain M1 targets, not a claim that restart restores a
production ratchet. WP-C is owner-accepted only as a bounded recovery foundation
with conditions/limitations: implementation `t_e3a1ba25` run 229; independent QA
`t_e022f9b4` run 230 PASS; final Evaluator `t_10759b1a` run 231
PASS-WITH-CONDITIONS. The [development roadmap](../KAMI_DEVELOPMENT_ROADMAP.md)
separates this acceptance from WP-A, WP-B and the incomplete KAMI-51 parent.

- Mutable account/prekey/session authority is process-local and default-deny on
  every storage-worker create/open. SQLCipher rows, generation counters, digests,
  clean-shutdown or quarantine/retirement records, ordinary `VerifiedEnrollment`,
  `create=true`, a new session ID or a new transport connection cannot prove freshness.
- Only identity's native one-use `FreshEnrollment` worker-local handoff can admit
  a freshly enrolled worker. It checks profile binding and absence of prior crypto/
  enrollment state, verifies the actual encrypted database under its new native
  key, durably commits/readbacks enrollment, completes native custody and verifies
  exact readback before admission. Failure quarantines that worker; guarded
  ambiguous I/O/corrupt outcomes also quarantine it. Native completion records
  alone cannot readmit it after restart.
- Exact committed ciphertext, inbox/frame records, receipts, retry artifacts and
  Delivered projections can reconcile from coherent durable evidence with current
  trust/pin/context checks, without re-encryption, duplicate ratchet advancement
  or missing-row repair. New crypto transitions remain fenced; recovery does not
  grant new publication/retention/ACK authority or expose available prekey inventory.
- A coherent older DB+WAL, including one predating exposure, consumption,
  retirement or quarantine, still starts Disabled. **Rollback detection is not
  available:** no independent freshness witness establishes the newest coherent
  database. Safety is implemented by refusing unsafe mutable restoration, not by
  pretending database-local evidence detects rollback.
- Typed `ReestablishmentPreconditions` require independently fresh enrollment,
  fresh account/prekeys, new session identity and independently verified contact.
  They are requirements, not live fresh-session establishment or authorization to
  repair an old profile. E2EE `ProfileRestore`/epoch fences remain fail-closed.

Reused independent QA evidence is 284 workspace passes, 0 failures and 3 existing
ignored tests; focused recovery has 6 passes. Formatting, locked all-target check,
warnings-denied Clippy and diff checks passed. Final evaluation independently
checked focused recovery/E2EE/runtime behavior and accepted only this bounded
contract. Native positives use disposable Linux private-D-Bus stores; Windows/
macOS, cross-network, interoperability, sustained fuzz/DoS, load and owner-operated
WP-C endpoint evidence remain unqualified. No independent rollback witness,
transparent mutable-session restoration, live fresh-session establishment,
production E2EE/application messaging, KAMI-28, WP-D or M1 completion is supplied.

## 10. Current bounded WP-D Slice 1 runtime-ingress foundation

WP-D Slice 1 is accepted only as a bounded, non-activated runtime-authenticated-
ingress foundation. A genuine `AuthenticatedConnection` is non-forgeable and is
retained by the same `Engine` that owns the runtime capability; caller-supplied
Peer IDs, device claims, signatures, hashes, or database generations cannot
substitute for it. Every operation revalidates the live connection, device/Peer-ID
binding, revocation, expiry, durable trust, and pin state before serialized worker
admission. WP-C's default-deny recovery/quarantine fence remains in force, and a
lost or potentially mutating worker boundary returns truthful `Indeterminate`.

Evidence is implementation run 235, independent QA run 236 PASS, and final
Evaluator run 237 PASS-WITH-CONDITIONS. This foundation does not establish a
fresh E2EE session, application-message delivery, CLI messaging, full WP-D,
KAMI-28, production E2EE, or M1. WP-D Slice 2 remains pending and must start
from the next committed baseline.

## 11. Engineering-controlled design details

The Architect owns the explicit selected Rust crypto implementation, key formats, fingerprint encoding, binding proof, key-store adapters, nonce/message-key handling, persistent state transitions, and threat-model tests. Any proposed weakening of explicit verification, E2EE, platform coverage, or ratcheting requires escalation rather than silent substitution.
