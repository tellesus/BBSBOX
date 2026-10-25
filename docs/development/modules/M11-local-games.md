# M11 — Game host and local multiplayer example

Version 0.1.0 · 2026-10-25 · Tier 2 proposal · Tier 1 baseline 0.4.0.

**State:** Planned capability; implementation choices proposed. No implementation or passed checks claimed.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C01](../contracts/C01-storage-runtime.md), [C04](../contracts/C04-games.md).
Look up only decision rows T10 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R07, R20, R21, R26, R27, P17, AC01, AC10, AC21, AC22. **Target gate:** G1. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M02](../modules/M02-storage-jobs.md), [M03](../modules/M03-identity-access.md), [M05](../modules/M05-interface-themes.md), [M07](../modules/M07-directory-profiles.md), [M09](../modules/M09-clans-cohorts.md).

## Product outcome and boundaries

Make games part of the first useful node with one small local game and multiplayer example. Define passport/profile/group hooks now; actual cross-node portability arrives in M18.

## Recommended implementation

Use locally installed HTML/TypeScript game assets and an isolated worker with JSON HTTP/WebSocket messages. Start with a simple turn-based two-player demonstration; no particular fictional dungeon or game title is committed. The game owns rules/state and publishes optional character summaries. Core authenticates players and supplies scoped sessions plus local memberships. Avoid a general plugin marketplace or arbitrary code-loader in the core.

## Ownership and interfaces

Proposed crates/game-host/, games/demo/; game manifests, launch/session registry and demo game state. Each game owns schema migrations and idempotent command handling.

Paths above are proposed implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: install/launch a bundled game and scoped identity handshake.
2. B: two local players, reconnect and save/restart.
3. C: opt-in profile character display and local group lookup; define adapter stubs for M18.

## Completion evidence

- Two users play through the browser while a third uses local boards.
- A command retry changes game state once.
- Profile settings hide game/character summaries; local groups remain local.

## Decisions before completion

Approve initial demo scope and game-worker packaging in T10. Unsupported games must not display a portability claim.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). A passing fixture is evidence for this module, not approval of new product scope. See [coverage](../COVERAGE.md) when changing source IDs.
