# O03 — Neighborhood Exchange

Version 1.1.0 · 2026-10-25 · Locked development baseline · Product baseline 0.6.0.

**State:** Approved for implementation under U19; unstarted. Runtime and hardware evidence remains pending.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C02](../contracts/C02-identity-events.md), [C05](../contracts/C05-private-content.md).
Look up only decision rows T09 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R28, P12, R01, R08, R09. **Target gate:** G5. **Expansion acceptance:** AC25. Scope summaries below preserve these source IDs; reconcile conflicts with the current product requirements and user instructions.

**Prerequisites:** [M06](M06-boards-mail.md), [M07](M07-directory-profiles.md), [M08](M08-friends-events.md), [M15](M15-selected-federation.md).

## Product outcome and boundaries

Approved listings for lending, barter, giveaways, jobs, skills and mutual-aid/project requests.

## Locked implementation

Use typed listing events with owner, category, description, locality, availability, expiry and withdrawn/completed state. Use contact through local mail and explicit listing updates; no payment or trading engine is implied. Default local distribution; enable selected text listing federation explicitly at the node. Human expiry is display/lifecycle data with clock uncertainty, never a way to resurrect an old listing.

## Ownership and interfaces

Planned crates/exchange/; listing revisions, category metadata and availability projection; consumes profile links and contact interfaces.

Paths above are planned implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: implement the approved listing categories and local creation/discovery.
2. B: contact, mark completed/withdrawn and expiry handling.
3. C: optional selected federation only after M15 with stale-update fixtures.

## Completion evidence

- A delayed original cannot undo completion/withdrawal.
- A listing stays usable without peers and does not replicate file libraries.
- Private contact fields are not copied into a listing without a separate user choice.

## Locked defaults and verification

Approve all named listing categories with owner-managed open/reserved/completed/withdrawn states. Default local distribution, with explicit selected text syndication. Default expiry suggestion is 30 days; owners may choose a date or no expiry; clock uncertainty is shown. Contact uses local mail. Expiry never erases history or overrides an authored withdrawal. No payments/POS integration.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
