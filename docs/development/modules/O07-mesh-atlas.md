# O07 — Optional node and forum Atlas

Version 0.1.0 · 2026-10-25 · Tier 2 proposal · Tier 1 baseline 0.4.0.

**State:** Optional candidate; product approval required. No implementation or passed checks claimed.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C02](../contracts/C02-identity-events.md), [C03](../contracts/C03-exchange-delivery.md).
Look up only decision rows T09, T13 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** P16, P22, R02, R09, R14. **Target gate:** G5. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M15](../modules/M15-selected-federation.md), [M17](../modules/M17-peer-trust.md).

## Product outcome and boundaries

Conditional directory of participating nodes and advertised forums. It is separate from the required local user directory.

## Recommended implementation

Exchange signed opt-in node/forum advertisements containing public identity, description, contact, capabilities and observed availability. Start with explicit peer-supplied entries and local filtering. Show last-known evidence instead of global live status. A node can withdraw an advertisement; disconnected copies update when the withdrawal arrives.

## Ownership and interfaces

Proposed crates/atlas/; advertisement projection and local discovery preferences. No mandatory registry, global map or courier movement history.

Paths above are proposed implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: public Node Passport/advertisement schema and local display.
2. B: selected exchange and withdrawn/stale entry treatment.
3. C: optional browse by human-entered locality/category.

## Completion evidence

- Private forum metadata is absent unless explicitly advertised.
- An unavailable node does not prevent ordinary local use.
- A forged advertisement cannot replace another node identity.

## Decisions before completion

Choose publicly advertised fields, retention and location precision; no automatic geolocation is implied.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). A passing fixture is evidence for this module, not approval of new product scope. See [coverage](../COVERAGE.md) when changing source IDs.
