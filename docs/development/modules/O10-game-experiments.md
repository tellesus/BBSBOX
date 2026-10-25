# O10 — Cross-node play and signature dungeons

Version 1.0.1 · 2026-10-25 · Locked development baseline · Product baseline 0.5.1.

**State:** Approved for implementation under U19; unstarted. Runtime and hardware evidence remains pending.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C02](../contracts/C02-identity-events.md), [C04](../contracts/C04-games.md).
Look up only decision rows T10 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R28, P18, P19, D03, R20, R21. **Target gate:** G5. **Expansion acceptance:** AC32. Scope summaries below preserve these source IDs; reconcile conflicts with the current product requirements and user instructions.

**Prerequisites:** [M18](M18-game-passports.md).

## Product outcome and boundaries

Approved game-specific experiments: asynchronous inter-node competition and a node's signature dungeon/rewards. No universal game world is promised.

## Locked implementation

Choose one turn-based game adapter first. Signed moves/results carry game-specific causal rules and explicit timeout assumptions; a disconnected match may wait without blocking local play. Signature dungeon content is a local game package; its reward provenance travels only through a receiving game's supported Passport rules. Cross-node tournaments/shared worlds remain D03 until separately unparked.

## Ownership and interfaces

Planned games/experiments/; per-game state, move/result schema, dungeon content and reward recognition rules. Core exposes transport and identity only.

Paths above are planned implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: implement the two-node turn-based encounter and define its causal rules.
2. B: prove delayed/replayed moves or imported rewards.
3. C: implement the node-specific dungeon and reward adapter after the encounter works.

## Completion evidence

- Out-of-order game actions cannot award a reward twice.
- Receiver can reject an unsupported reward/state under its own rules.
- A node without this game continues normal community operation.

## Locked defaults and verification

Both P18 and P19 are approved. First add a correspondence-style two-node encounter to the demo adapter, then a host-customized local dungeon with signed reward provenance. Use causal moves and explicit waiting state; no automatic timeout while disconnected. Destination game policy accepts/rejects rewards. Shared worlds/tournaments remain D03.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
