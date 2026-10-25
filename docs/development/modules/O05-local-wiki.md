# O05 — Local knowledge and history wiki

Version 0.1.0 · 2026-10-25 · Tier 2 proposal · Tier 1 baseline 0.4.0.

**State:** Optional candidate; product approval required. No implementation or passed checks claimed.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C02](../contracts/C02-identity-events.md), [C05](../contracts/C05-private-content.md).
Look up only decision rows T05, T09 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** P14, R01, R08, R09. **Target gate:** G5. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M06](../modules/M06-boards-mail.md), [M07](../modules/M07-directory-profiles.md).

## Product outcome and boundaries

Conditional text knowledge base/local history with selected article synchronization.

## Recommended implementation

Prefer Markdown pages with immutable revisions and explicit parent revisions, a local index and revision comparison. Concurrent edits create visible branches for an editor to resolve. Do not add a general CRDT editor before a real concurrent-edit need. Text pages may later use M15 subscriptions; embedded local file references remain local.

## Ownership and interfaces

Proposed crates/wiki/; pages, revision graph, editorial permissions and selected sync subscriptions. Uses stable identities and event verification.

Paths above are proposed implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: create/read/edit local pages and history.
2. B: conflict/merge UI with two concurrent revisions.
3. C: selected text-page exchange after M15.

## Completion evidence

- Reordered edits cannot overwrite an unmerged branch unnoticed.
- A local attachment is not included automatically in a page sync.
- History remains attributable and navigable after restore.

## Decisions before completion

Choose editorial roles, page naming/rename rules and first federation scope.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). A passing fixture is evidence for this module, not approval of new product scope. See [coverage](../COVERAGE.md) when changing source IDs.
