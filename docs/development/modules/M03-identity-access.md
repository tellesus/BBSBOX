# M03 — Identity, local accounts and permissions

Version 0.1.0 · 2026-10-25 · Tier 2 proposal · Tier 1 baseline 0.4.0.

**State:** Planned capability; implementation choices proposed. No implementation or passed checks claimed.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C01](../contracts/C01-storage-runtime.md), [C02](../contracts/C02-identity-events.md).
Look up only decision rows T04, T08, T09 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R07, R13, R14, R17, P07, P22, AC01, AC11, AC12, AC14, AC15. **Target gate:** G1. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M00](../modules/M00-foundation-spikes.md), [M02](../modules/M02-storage-jobs.md).

## Product outcome and boundaries

Separate local usernames/sessions from portable people and node/forum/courier authority. A SysOp can administer accounts without acquiring private group credentials.

## Recommended implementation

Propose local username/password sessions with Argon2id password hashes, secure same-origin cookies and CSRF protection for ordinary browser login. User root/device credentials are separate client-held keys; use a maintained crypto implementation. Root-authorized device records support introductions, rotation and prepared recovery. QR/fingerprint cards convey public identity, not recovery secrets. Device roots and node signing incarnations prevent replay after restore.

## Ownership and interfaces

Proposed crates/identity/ and client/identity/; accounts, sessions, roles, public certificates, device delegations, contact cards and encrypted client recovery exports. Core owns the public resolver/authorize API.

Paths above are proposed implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: local account/login/role boundaries and bootstrap SysOp.
2. B: independent person/device/node/forum identifiers and public introductions.
3. C: device replacement, account linking at a destination and offline recovery-kit exercise.

## Completion evidence

- A username collision across nodes never merges two people.
- Login recovery does not claim to recover lost E2E keys; verify both paths separately.
- A restored signer cannot accidentally append to a counter position already used by its live predecessor.

## Decisions before completion

Finalize recovery-kit format and root-loss policy; propose root-signed device rotation, not social/quorum recovery in this release. Freeze device admission and account-linking UX before federation.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). A passing fixture is evidence for this module, not approval of new product scope. See [coverage](../COVERAGE.md) when changing source IDs.
