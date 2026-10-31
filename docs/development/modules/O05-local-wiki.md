# O05 — Local knowledge and history wiki

Version 1.1.0 · 2026-10-25 · Locked development baseline · Product baseline 0.6.0.

**State:** Approved for implementation under U19; unstarted. Runtime and hardware evidence remains pending.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C02](../contracts/C02-identity-events.md), [C05](../contracts/C05-private-content.md).
Look up only decision rows T05, T09 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R28, P14, R01, R08, R09. **Target gate:** G5. **Expansion acceptance:** AC27. Scope summaries below preserve these source IDs; reconcile conflicts with the current product requirements and user instructions.

**Prerequisites:** [M06](M06-boards-mail.md), [M07](M07-directory-profiles.md), [M15](M15-selected-federation.md).

## Product outcome and boundaries

Approved text knowledge base/local history with selected article synchronization.

## Locked implementation

Use Markdown pages with immutable revisions and explicit parent revisions, a local index and revision comparison. Concurrent edits create visible branches for an editor to resolve. Do not add a general CRDT editor before a real concurrent-edit need. Text pages may later use M15 subscriptions; embedded local file references remain local.

## Ownership and interfaces

Planned crates/wiki/; pages, revision graph, editorial permissions and selected sync subscriptions. Uses stable identities and event verification.

Paths above are planned implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: create/read/edit local pages and history.
2. B: conflict/merge UI with two concurrent revisions.
3. C: selected text-page exchange after M15.

## Completion evidence

- Reordered edits cannot overwrite an unmerged branch unnoticed.
- A local attachment is not included automatically in a page sync.
- History remains attributable and navigable after restore.

## Locked defaults and verification

Local members submit page revisions; delegated editors publish/merge them. Pages have stable IDs with rename aliases; concurrent edits remain branches until explicitly merged. Text-only selected syndication is approved; local files remain references. Use Markdown and revision comparison, not a general CRDT collaborative editor in this baseline.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
