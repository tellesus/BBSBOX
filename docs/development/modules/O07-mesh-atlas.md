# O07 — Optional node and forum Atlas

Version 1.1.0 · 2026-10-25 · Locked development baseline · Product baseline 0.6.0.

**State:** Approved for implementation under U19; unstarted. Runtime and hardware evidence remains pending.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C02](../contracts/C02-identity-events.md), [C03](../contracts/C03-exchange-delivery.md).
Look up only decision rows T09, T13 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R28, P16, P22, R02, R09, R14. **Target gate:** G5. **Expansion acceptance:** AC29. Scope summaries below preserve these source IDs; reconcile conflicts with the current product requirements and user instructions.

**Prerequisites:** [M15](M15-selected-federation.md), [M17](M17-peer-trust.md).

## Product outcome and boundaries

Approved directory of participating nodes and advertised forums. It is separate from the required local user directory.

## Locked implementation

Exchange signed opt-in node/forum advertisements containing public identity, description, contact, capabilities and observed availability. Start with explicit peer-supplied entries and local filtering. Show last-known evidence instead of global live status. A node can withdraw an advertisement; disconnected copies update when the withdrawal arrives.

## Ownership and interfaces

Planned crates/atlas/; advertisement projection and local discovery preferences. No mandatory registry, global map or courier movement history.

Paths above are planned implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: public Node Passport/advertisement schema and local display.
2. B: selected exchange and withdrawn/stale entry treatment.
3. C: optional browse by human-entered locality/category.

## Completion evidence

- Private forum metadata is absent unless explicitly advertised.
- An unavailable node does not prevent ordinary local use.
- A forged advertisement cannot replace another node identity.

## Locked defaults and verification

Atlas uses opt-in signed node/forum advertisements: identity, name, description, chosen operator contact, capabilities, approximate human-entered locality and category. No coordinates or location lookup by default. Entries show last-known evidence and withdrawals; local SysOps can remove them. No global registry or public private-forum metadata.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
