# M14 — Targeted complaints and host suppression

Version 0.1.0 · 2026-10-25 · Tier 2 proposal · Tier 1 baseline 0.4.0.

**State:** Planned capability; implementation choices proposed. No implementation or passed checks claimed.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C02](../contracts/C02-identity-events.md), [C05](../contracts/C05-private-content.md).
Look up only decision rows T09, T13 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R13, R16, R17, P03, AC13, AC19. **Target gate:** G2. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M06](../modules/M06-boards-mail.md), [M13](../modules/M13-encrypted-forums.md).

## Product outcome and boundaries

Preserve the original complaint, exact posts/revisions and explanation through SysOp contact and moderator response. A user may allege illegality; only a SysOp chooses local temporary suppression.

## Recommended implementation

Implement the C05 case state machine with stable case/target IDs and role-filtered views. Route contact through local mail; use permitted signed exchange for remote cases once M15/M16 exist. A review endpoint serves suppressed ciphertext only after forum-role proof; group keys remain client-side. Host suppression applies to ordinary delivery/read routes at that node and does not grant membership or compel other nodes.

## Ownership and interfaces

Proposed crates/complaints/; case records, target/revision links, forward/response events, local suppression flags and local-resolution actions. Forum content stays owned by M06/M13.

Paths above are proposed implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: report one/multiple targets, preserve explanation/revision and forward case.
2. B: authorized moderator retrieval while suppressed, reply and local resolution.
3. C: edited/missing target and delayed remote response fixtures.

## Completion evidence

- AC13/AC19 from reporter, host-only SysOp and member-moderator views.
- An allegation alone changes no visibility; removing suppression restores permitted ordinary access.
- Wrong-role access cannot reveal ciphertext via the review bypass; missing target is shown as pending rather than silently substituted.

## Decisions before completion

Decide case/revision retention and reporter-visible response fields; voluntary plaintext disclosure remains a separate P03 choice.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). A passing fixture is evidence for this module, not approval of new product scope. See [coverage](../COVERAGE.md) when changing source IDs.
