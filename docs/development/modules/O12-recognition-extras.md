# O12 — Optional recognition and profile/group additions

Version 1.0.1 · 2026-10-25 · Locked development baseline · Product baseline 0.5.1.

**State:** Approved for implementation under U19; unstarted. Runtime and hardware evidence remains pending.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C04](../contracts/C04-games.md), [C05](../contracts/C05-private-content.md).
Look up only decision rows T09, T10, T11 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R28, P23, P24, R25, R26, R27. **Target gate:** G5. **Expansion acceptance:** AC34. Scope summaries below preserve these source IDs; reconcile conflicts with the current product requirements and user instructions.

**Prerequisites:** [M07](M07-directory-profiles.md), [M08](M08-friends-events.md), [M09](M09-clans-cohorts.md), [M11](M11-local-games.md), [M16](M16-pigeons-lump-drops.md), [O11](O11-business-rewards.md).

## Product outcome and boundaries

Two approved feature tracks: discretionary SysOp rewards for Lump Drops (P23), and opt-in game achievements/progress or additional group/profile features (P24). M16 remains count-only; O12 supplies the approved reward/display extensions.

## Locked implementation

For P23, propose manual SysOp grants linked to a selected local credit summary and an idempotent grant record; no automatic reward schedule. For P24, use versioned game-provided achievement/progress summaries with user opt-in. A possible group activity panel should reuse permitted events/game summaries within the approved P24 scope. Avoid a general ranking system.

## Ownership and interfaces

Planned crates/recognition/ and profile providers; grant records and display preferences. Original credits, game achievements and group membership stay owned by their source modules.

Paths above are planned implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: implement opt-in achievement/progress providers.
2. B: implement manual discretionary grants and exactly-once command handling within intact durable state.
3. C: implement the local group activity panel with source-audience checks.

## Completion evidence

- Private Lump Drop counts and game progress stay hidden in all public components.
- A grant retry does not duplicate a game reward.
- Installing display extras does not silently enable rewards or federate local groups.

## Locked defaults and verification

Approve manual discretionary Lump Drop grants, opt-in achievement/progress components and a local group activity panel using already permitted event/game summaries. No automatic reward schedule or leaderboard. Grants use idempotent IDs through C04. Implement display providers first, manual rewards second, group panel third. A private count remains hidden even when the user receives a grant.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
