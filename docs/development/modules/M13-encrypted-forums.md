# M13 — Trusted clients and encrypted forums

Version 0.1.0 · 2026-10-25 · Tier 2 proposal · Tier 1 baseline 0.4.0.

**State:** Planned capability; implementation choices proposed. No implementation or passed checks claimed.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C02](../contracts/C02-identity-events.md), [C05](../contracts/C05-private-content.md).
Look up only decision rows T06, T07, T08 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R13, R15, R17, P07, P09, AC11, AC14, AC15. **Target gate:** G2. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M00](../modules/M00-foundation-spikes.md), [M04](../modules/M04-signed-events.md), [M05](../modules/M05-interface-themes.md), [M06](../modules/M06-boards-mail.md), [M12](../modules/M12-maintenance-recovery.md).

## Product outcome and boundaries

Members decrypt private forum content; hosting/carriage does not grant plaintext. Disconnected stale-credential access is accepted; no network-wide posting halt.

## Recommended implementation

Prefer a vetted MLS implementation, first evaluating OpenMLS shared Rust/native and WASM crypto, with a separately trusted signed client bundle where needed. Keep MLS key packages/epochs on member clients and opaque payloads on hosts. Propose owner-authorized serialized governance commits with delegated moderator actions; ordinary posting uses the client's locally known valid epoch. Handle stale epochs, commit branches and retention according to measured library behavior, not improvised cryptography. Exact outer admission proofs must let a host validate a new encrypted event without learning its content.

## Ownership and interfaces

Proposed crates/private-forum/ and client/crypto/; encrypted event kinds, public authority proofs, client group/key state, invitations and encrypted client backups. Hosts own ciphertext indexes only.

Paths above are proposed implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: finalize client trust and MLS decision from M00 evidence; freeze metadata/admission contract.
2. B: create/invite/post/read/remove with two clients and no host keys.
3. C: prolonged disconnection, concurrent governance, old-epoch messages and restore/rejoin.

Later integration is owned by M15/M16: repeat these checks over real transport before federation is called private. This is not a prerequisite for completing the encrypted-local module.

## Completion evidence

- AC11 includes a hosting-only SysOp and carrier that cannot decrypt or replace trusted client code undetected under the selected model.
- AC14 preserves accepted stale-state behavior without rejecting valid unrelated offline work.
- A fresh relevant encrypted event is recognizable at its authenticated envelope boundary for M16; unknown authority remains pending.

## Decisions before completion

T06/T07 are release gates: library fit, iOS/Android/browser delivery, metadata leakage, old-epoch retention and branch recovery. If a supported platform is unproven, record that limitation and keep its encrypted feature gate open.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). A passing fixture is evidence for this module, not approval of new product scope. See [coverage](../COVERAGE.md) when changing source IDs.
