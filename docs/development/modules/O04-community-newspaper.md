# O04 — Community newspaper and print editions

Version 1.1.0 · 2026-10-25 · Locked development baseline · Product baseline 0.6.0.

**State:** Approved for implementation under U19; unstarted. Runtime and hardware evidence remains pending.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C02](../contracts/C02-identity-events.md), [C05](../contracts/C05-private-content.md).
Look up only decision rows T09 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R28, P13, R01, R08, R09. **Target gate:** G5. **Expansion acceptance:** AC26. Scope summaries below preserve these source IDs; reconcile conflicts with the current product requirements and user instructions.

**Prerequisites:** [M06](M06-boards-mail.md), [M07](M07-directory-profiles.md), [M15](M15-selected-federation.md).

## Product outcome and boundaries

Approved local publishing, printable editions and selected article syndication.

## Locked implementation

Use curated article references, immutable edition manifests and browser print CSS. Begin with printable HTML; evaluate an offline PDF renderer only if print trials need it. An editor selects articles whose publication audience allows inclusion. Syndicate selected text through M15; local library images/assets are not automatically copied.

## Ownership and interfaces

Planned crates/newspaper/; editorial drafts, article selections and published edition manifests. Canonical post rights/audience remain with their owning modules.

Paths above are planned implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: local article selection and preview.
2. B: freeze/print an edition and publish corrections as a later edition.
3. C: optional text syndication after M15 and explicit policy.

## Completion evidence

- Print output includes all selected content and usable links/identifiers.
- A private forum post cannot enter a public edition merely because an editor knows its ID.
- Opening an old edition does not silently substitute a newer article revision.

## Locked defaults and verification

Editors appointed by the SysOp curate permitted public articles into immutable editions; corrections create a new edition. Use HTML print CSS and browser print-to-PDF; no separate PDF service. Selected text syndication and a manual paper-flyer-to-post form are approved; automatic OCR is not assumed. External images are not fetched; local image rendering stays local.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
