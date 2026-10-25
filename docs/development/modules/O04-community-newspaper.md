# O04 — Community newspaper and print editions

Version 0.1.0 · 2026-10-25 · Tier 2 proposal · Tier 1 baseline 0.4.0.

**State:** Optional candidate; product approval required. No implementation or passed checks claimed.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C02](../contracts/C02-identity-events.md), [C05](../contracts/C05-private-content.md).
Look up only decision rows T09 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** P13, R01, R08, R09. **Target gate:** G5. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M06](../modules/M06-boards-mail.md), [M07](../modules/M07-directory-profiles.md).

## Product outcome and boundaries

Conditional local publishing, printable editions and selected article syndication.

## Recommended implementation

Use curated article references, immutable edition manifests and browser print CSS. Begin with printable HTML; evaluate an offline PDF renderer only if print trials need it. An editor selects articles whose publication audience allows inclusion. Syndicate selected text through M15; local library images/assets are not automatically copied.

## Ownership and interfaces

Proposed crates/newspaper/; editorial drafts, article selections and published edition manifests. Canonical post rights/audience remain with their owning modules.

Paths above are proposed implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: local article selection and preview.
2. B: freeze/print an edition and publish corrections as a later edition.
3. C: optional text syndication after M15 and explicit policy.

## Completion evidence

- Print output includes all selected content and usable links/identifiers.
- A private forum post cannot enter a public edition merely because an editor knows its ID.
- Opening an old edition does not silently substitute a newer article revision.

## Decisions before completion

Choose editorial roles, allowed media and correction/edition policy; PDF and syndication are separate decisions.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). A passing fixture is evidence for this module, not approval of new product scope. See [coverage](../COVERAGE.md) when changing source IDs.
