# O03 — Neighborhood Exchange

Version 0.1.0 · 2026-10-25 · Tier 2 proposal · Tier 1 baseline 0.4.0.

**State:** Optional candidate; product approval required. No implementation or passed checks claimed.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C02](../contracts/C02-identity-events.md), [C05](../contracts/C05-private-content.md).
Look up only decision rows T09 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** P12, R01, R08, R09. **Target gate:** G5. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M07](../modules/M07-directory-profiles.md), [M08](../modules/M08-friends-events.md).

## Product outcome and boundaries

Conditional listings for lending, barter, giveaways, jobs, skills and mutual-aid/project requests.

## Recommended implementation

Use typed listing events with owner, category, description, locality, availability, expiry and withdrawn/completed state. Prefer contact through local mail and explicit listing updates; no payment or trading engine is implied. Default local distribution; separately choose selected text listing federation. Human expiry is display/lifecycle data with clock uncertainty, never a way to resurrect an old listing.

## Ownership and interfaces

Proposed crates/exchange/; listing revisions, category metadata and availability projection; consumes profile links and contact interfaces.

Paths above are proposed implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: approve a small set of listing types and local creation/discovery.
2. B: contact, mark completed/withdrawn and expiry handling.
3. C: optional selected federation only after M15 with stale-update fixtures.

## Completion evidence

- A delayed original cannot undo completion/withdrawal.
- A listing stays usable without peers and does not replicate file libraries.
- Private contact fields are not copied into a listing without a separate user choice.

## Decisions before completion

Select categories, availability lifecycle, expiry UI and distribution before adding this module.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). A passing fixture is evidence for this module, not approval of new product scope. See [coverage](../COVERAGE.md) when changing source IDs.
