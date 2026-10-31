# M03 — Identity, local accounts and permissions

Brief revision 1.1.3 · Roadmap baseline 1.1.0 · 2026-10-31 · Product baseline 0.6.0.

**State:** In progress, validating C under U22. A/B passed Windows/Linux; A–C have local crypto/account/recovery evidence. C Linux and consumer integration remain pending. Consume C01 v1.3.0/C02 v1.3.0.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C01](../contracts/C01-storage-runtime.md), [C02](../contracts/C02-identity-events.md).
Look up only decision rows T04, T08, T09 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R07, R13, R14, R17, P07, P22, AC01, AC11, AC12, AC14, AC15. **Target gate:** G1. Scope summaries below preserve these source IDs; reconcile conflicts with the current product requirements and user instructions.

**Prerequisites:** [M00](M00-foundation-spikes.md), [M02](M02-storage-jobs.md).

M00 supplies G0-local here; actual private-client acceptance remains G2.

## Product outcome and boundaries

Separate local usernames/sessions from portable people and node/forum/courier authority. A SysOp can administer accounts without acquiring private group credentials.

## Locked implementation

Use local username/password sessions with Argon2id password hashes, secure same-origin cookies and CSRF protection for ordinary browser login. User root/device credentials are separate client-held keys; use a maintained crypto implementation. Root-authorized device records support introductions, rotation and prepared recovery. QR/fingerprint cards convey public identity, not recovery secrets. Device roots and node signing incarnations prevent replay after restore.

## Ownership and interfaces

crates/identity/ implements accounts, public resolution, destination linking and node signer lifecycle. crates/credentials/ verifies proofs; client/identity/ owns signing/recovery. Core supplies authorization services; features check resource authority. See [accounts](../../../crates/identity/README.md) and [credential profile](../../../crates/credentials/README.md).

M05 owns HTTP/enrollment UI integration; A exposes authenticated SysOp account creation without a public bootstrap route. Consume listed contracts and update affected consumers together.

## Sequential work slices

1. A: local account/login/role boundaries and bootstrap SysOp.
2. B: independent person/device/node/forum identifiers and public introductions.
3. C: device replacement, account linking at a destination and offline recovery-kit exercise.

## Completion evidence

- A username collision across nodes never merges two people.
- Login recovery does not claim to recover lost E2E keys; verify both paths separately.
- A restored signer cannot accidentally append to a counter position already used by its live predecessor.

## Locked defaults and verification

Local account/password sessions and separate root-authorized device credentials are selected. Recovery is an encrypted user-held kit containing the root and necessary client state; passwords alone recover no E2E keys. Use established authenticated encryption and password derivation libraries. Public QR/fingerprint introductions contain no secret. Every restore creates a new authorized writer incarnation; root loss with no kit creates a new identity. Device addition requires existing root authorization.

A defaults: case-folded 1–32 ASCII letters/digits/underscore/hyphen usernames; passwords 15+ characters/1,024-byte maximum, Argon2id 19 MiB/two passes/one lane. Two password slots, ten attempts/username/minute, 1,024 limiter entries. Secure host-only session cookies use HttpOnly/SameSite=Strict plus origin/CSRF checks. Sessions use monotonic seven-day absolute/twelve-hour idle expiry, 32/account maximum, and end on restart. Role/password changes revoke sessions; preserve the last active SysOp. Never persist password/session secrets in receipts or plain request fingerprints.

C defaults: recovery profile 1 uses Argon2id 64 MiB/three passes/one lane and XChaCha20Poly1305; up to 8 MiB supplied client state. Restore creates fresh devices. Linking requires the current account session plus a node/account/nonce-bound root proof; challenges expire after five minutes, maximum eight pending/account. Explicit unlink preserves signed history. Node seed/public identity commit together; each startup changes incarnation and restored storage blocks signing pending M12 recovery.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
