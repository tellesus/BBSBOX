# O06 — Publisher-controlled Meshcasts

Version 1.0.1 · 2026-10-25 · Locked development baseline · Product baseline 0.5.1.

**State:** Approved for implementation under U19; unstarted. Runtime and hardware evidence remains pending.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C02](../contracts/C02-identity-events.md), [C03](../contracts/C03-exchange-delivery.md).
Look up only decision rows T05, T09 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R28, P15, R09, R18, R19. **Target gate:** G5. **Expansion acceptance:** AC28. Scope summaries below preserve these source IDs; reconcile conflicts with the current product requirements and user instructions.

**Prerequisites:** [M08](M08-friends-events.md), [M15](M15-selected-federation.md).

## Product outcome and boundaries

Approved compact publisher-controlled announcement feeds, building on already required chosen feeds.

## Locked implementation

Reuse signed post/event records with a publisher manifest, explicit subscribers and authorized publisher keys. Add topic/feed descriptions and delivery budgets only where the general feed module is insufficient. Preserve chronological ordering and late-arrival indication. No recommendation ranking or paid reach.

## Ownership and interfaces

Planned crates/meshcasts/; publisher manifests and feed subscription views; core posts and transport remain owned elsewhere.

Paths above are planned implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: specify the publisher-manifest specialization over M08/M15; reuse their storage and transport.
2. B: publish/subscribe over selected links.
3. C: key rotation, withdrawal and delayed delivery.

## Completion evidence

- Only authorized publishers update the feed.
- Old announcements cannot undo corrections.
- Unsubscribing changes future delivery according to explicit local retention policy.

## Locked defaults and verification

Implement Meshcasts as the M08/M15 feed specialization with publisher manifest, explicit subscribers and key rotation, not another independent storage/transport service. Local configuration controls publication and delivery budget. The module completes when the specialization works, even if it adds little separate code.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
