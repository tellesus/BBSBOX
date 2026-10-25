# O06 — Publisher-controlled Meshcasts

Version 0.1.0 · 2026-10-25 · Tier 2 proposal · Tier 1 baseline 0.4.0.

**State:** Optional candidate; product approval required. No implementation or passed checks claimed.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C02](../contracts/C02-identity-events.md), [C03](../contracts/C03-exchange-delivery.md).
Look up only decision rows T05, T09 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** P15, R09, R18, R19. **Target gate:** G5. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M08](../modules/M08-friends-events.md), [M15](../modules/M15-selected-federation.md).

## Product outcome and boundaries

Conditional compact publisher-controlled announcement feeds, building on already required chosen feeds.

## Recommended implementation

Reuse signed post/event records with a publisher manifest, explicit subscribers and authorized publisher keys. Add topic/feed descriptions and delivery budgets only where the general feed module is insufficient. Preserve chronological ordering and late-arrival indication. No recommendation ranking or paid reach.

## Ownership and interfaces

Proposed crates/meshcasts/; publisher manifests and feed subscription views; core posts and transport remain owned elsewhere.

Paths above are proposed implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: document the behavior missing from M08 and approve only that difference.
2. B: publish/subscribe over selected links.
3. C: key rotation, withdrawal and delayed delivery.

## Completion evidence

- Only authorized publishers update the feed.
- Old announcements cannot undo corrections.
- Unsubscribing changes future delivery according to explicit local retention policy.

## Decisions before completion

Confirm this warrants a distinct module; use existing M08/M15 configuration if it satisfies the use case.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). A passing fixture is evidence for this module, not approval of new product scope. See [coverage](../COVERAGE.md) when changing source IDs.
