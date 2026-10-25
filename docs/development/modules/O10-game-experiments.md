# O10 — Cross-node play and signature dungeons

Version 0.1.0 · 2026-10-25 · Tier 2 proposal · Tier 1 baseline 0.4.0.

**State:** Optional candidate; product approval required. No implementation or passed checks claimed.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C02](../contracts/C02-identity-events.md), [C04](../contracts/C04-games.md).
Look up only decision rows T10 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** P18, P19, D03, R20, R21. **Target gate:** G5. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M18](../modules/M18-game-passports.md).

## Product outcome and boundaries

Conditional game-specific experiments: asynchronous inter-node competition and a node's signature dungeon/rewards. No universal game world is promised.

## Recommended implementation

Choose one turn-based game adapter first. Signed moves/results carry game-specific causal rules and explicit timeout assumptions; a disconnected match may wait without blocking local play. Signature dungeon content is a local game package; its reward provenance travels only through a receiving game's supported Passport rules. Cross-node tournaments/shared worlds remain D03 until separately unparked.

## Ownership and interfaces

Proposed games/experiments/; per-game state, move/result schema, dungeon content and reward recognition rules. Core exposes transport and identity only.

Paths above are proposed implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: select either a small match or signature dungeon and define rules.
2. B: prove delayed/replayed moves or imported rewards.
3. C: evaluate another experiment only after the first has a usable result.

## Completion evidence

- Out-of-order game actions cannot award a reward twice.
- Receiver can reject an unsupported reward/state under its own rules.
- A node without this game continues normal community operation.

## Decisions before completion

Approve P18 and P19 independently; shared worlds and exclusive global economies are not inferred.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). A passing fixture is evidence for this module, not approval of new product scope. See [coverage](../COVERAGE.md) when changing source IDs.
