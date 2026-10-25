# O12 — Optional recognition and profile/group additions

Version 0.1.0 · 2026-10-25 · Tier 2 proposal · Tier 1 baseline 0.4.0.

**State:** Optional candidate; product approval required. No implementation or passed checks claimed.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C04](../contracts/C04-games.md), [C05](../contracts/C05-private-content.md).
Look up only decision rows T09, T10, T11 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** P23, P24, R25, R26, R27. **Target gate:** G5. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M07](../modules/M07-directory-profiles.md), [M09](../modules/M09-clans-cohorts.md), [M11](../modules/M11-local-games.md), [M16](../modules/M16-pigeons-lump-drops.md).

## Product outcome and boundaries

Two independent proposal tracks: discretionary SysOp rewards for Lump Drops (P23), and opt-in game achievements/progress or additional group/profile features (P24). Current R25 remains count-only.

## Recommended implementation

For P23, propose manual SysOp grants linked to a selected local credit summary and an idempotent grant record; no automatic reward schedule. For P24, use versioned game-provided achievement/progress summaries with user opt-in. A possible group activity panel should reuse permitted events/game summaries and receive its own scope decision. Avoid a general ranking system.

## Ownership and interfaces

Proposed crates/recognition/ and profile providers; grant records and display preferences. Original credits, game achievements and group membership stay owned by their source modules.

Paths above are proposed implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: choose P23 or a named P24 addition; record separate approval.
2. B: implement one adapter/provider and visibility rules.
3. C: evaluate further additions independently.

## Completion evidence

- Private Lump Drop counts and game progress stay hidden in all public components.
- A grant retry does not duplicate a game reward.
- Installing display extras does not silently enable rewards or federate local groups.

## Decisions before completion

No unspecified fun feature is approved by this brief. Select exactly which display or reward capability to add.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). A passing fixture is evidence for this module, not approval of new product scope. See [coverage](../COVERAGE.md) when changing source IDs.
