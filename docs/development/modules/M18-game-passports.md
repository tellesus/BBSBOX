# M18 — Portable characters and Game Passports

Version 1.0.1 · 2026-10-25 · Locked development baseline · Product baseline 0.5.1.

**State:** Approved for implementation under U19; unstarted. Runtime and hardware evidence remains pending.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C02](../contracts/C02-identity-events.md), [C04](../contracts/C04-games.md).
Look up only decision rows T10 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R20, R21, R26, R27, P17, AC10, AC21, AC22. **Target gate:** G4. Scope summaries below preserve these source IDs; reconcile conflicts with the current product requirements and user instructions.

**Prerequisites:** [M11](M11-local-games.md), [M15](M15-selected-federation.md), [M16](M16-pigeons-lump-drops.md).

## Product outcome and boundaries

Move supported character/account state between nodes with destination game validation and configurable GM approval. Preserve local-only clan/cohort semantics.

## Locked implementation

Implement C04 signed snapshot exports, adapter-owned schema validation, staged import, GM decision and idempotent acceptance. Use explicit copy/import branches first; the UI must describe that both origin and destination may retain playable branches. Destination rules can accept/reject rewards. Existing BBS transport carries only selected game records; large assets remain installed locally. A saved export can be inspected without the source node, but fresh ownership/revocation knowledge remains bounded by available evidence.

## Ownership and interfaces

Planned crates/passport/ and game-specific adapters; export/import receipts, origin/branch provenance and GM decisions. Each game owns state conversion; core owns identity verification and routing.

Paths above are planned implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: same-version export/import for the demo game and opt-in profile summary.
2. B: owner-authorized destination-bound import, GM approval, duplicate import and unsupported schema.
3. C: source-offline/source-lost import of a saved snapshot and divergent re-export.
4. D: verify local group association at destination.

## Completion evidence

- AC10/AC21/AC22 across two independent BBSes.
- No portable claim for an adapter without passing fixtures.
- Neither old group membership nor game purchase grants destination enrollment/social reach.

## Locked defaults and verification

First Passport adapters use copy/import branches with explicit origin/destination lineage, not exclusive transfer. GM approval defaults on for imported characters and may be configured per game. Imports carry a game-issued export plus owner authorization bound to destination, game, export and a fresh challenge; a stolen snapshot cannot claim someone else’s character. Duplicate import ID returns the original result. Large local game assets stay separately installed; set snapshot cap in the adapter schema.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
