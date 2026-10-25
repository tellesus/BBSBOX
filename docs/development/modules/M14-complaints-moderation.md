# M14 — Targeted complaints and host suppression

Version 1.0.0 · 2026-10-25 · Locked development baseline · Tier 1 baseline 0.5.0.

**State:** Approved for implementation under U19; unstarted. Runtime and hardware evidence remains pending.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C02](../contracts/C02-identity-events.md), [C05](../contracts/C05-private-content.md).
Look up only decision rows T09, T13 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R13, R16, R17, P03, AC13, AC19. **Target gate:** G2. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M06](../modules/M06-boards-mail.md), [M13](../modules/M13-encrypted-forums.md).

## Product outcome and boundaries

Preserve the original complaint, exact posts/revisions and explanation through SysOp contact and moderator response. A user may allege illegality; only a SysOp chooses local temporary suppression.

## Locked implementation

Implement the C05 case state machine with stable case/target IDs and role-filtered views. Route contact through local mail; use permitted signed exchange for remote cases once M15/M16 exist. A review endpoint serves suppressed ciphertext only after forum-role proof; group keys remain client-side. Host suppression applies to ordinary delivery/read routes at that node and does not grant membership or compel other nodes.

## Ownership and interfaces

Planned crates/complaints/; case records, target/revision links, forward/response events, local suppression flags and local-resolution actions. Forum content stays owned by M06/M13.

Paths above are planned implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: report one/multiple targets, preserve explanation/revision and forward case.
2. B: authorized moderator retrieval while suppressed, reply and local resolution.
3. C: edited/missing target and delayed remote response fixtures.

## Completion evidence

- AC13/AC19 from reporter, host-only SysOp and member-moderator views.
- An allegation alone changes no visibility; removing suppression restores permitted ordinary access.
- Wrong-role access cannot reveal ciphertext via the review bypass; missing target is shown as pending rather than silently substituted.

## Locked defaults and verification

Keep complaint targets/revisions for the open case plus 90 days after local closure, configurable with visible missing-target state. Reporter sees status and response intended for them; SysOp sees explanation/case state; authorized mod sees target plaintext on their client. Voluntary plaintext disclosure is approved as an explicit opt-in attachment owned here. Suppression covers ordinary node reads, searches, exports and sync responses; the scoped moderator-review path remains usable.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved; genuinely new scope still follows change control. See [coverage](../COVERAGE.md) when changing source IDs.
