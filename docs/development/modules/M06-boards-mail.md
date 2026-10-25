# M06 — Local boards and private mail

Version 0.1.0 · 2026-10-25 · Tier 2 proposal · Tier 1 baseline 0.4.0.

**State:** Planned capability; implementation choices proposed. No implementation or passed checks claimed.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C02](../contracts/C02-identity-events.md), [C05](../contracts/C05-private-content.md).
Look up only decision rows T05, T09 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R07, R12, R13, P03, AC01, AC02, AC07. **Target gate:** G1. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M04](../modules/M04-signed-events.md), [M05](../modules/M05-interface-themes.md).

## Product outcome and boundaries

Deliver local threaded public discussion and recipient-restricted local mail, including operator contact. This module does not claim host-blind mail or private encrypted forums; M13 and optional O02 supply those distinct capabilities.

## Recommended implementation

Use immutable post/message events and indexed thread/mailbox projections. Author edits reference immutable revisions; tombstones preserve causal ordering. Render a bounded Markdown subset with sanitized output and no executable embedded content. Local mail uses recipient access controls; label its host-visible storage model accurately. File attachments are excluded from the first slice pending an explicit attachment decision, while local file areas remain available via M10.

## Ownership and interfaces

Proposed crates/discussions/ and crates/mail/; thread/post revisions, mailboxes, reply links, recipient policies and operator contacts. Both expose stable post/message references to complaints and sync.

Paths above are proposed implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: create/read/reply boards with explicit local-only distribution.
2. B: edit/withdraw/replay behavior and stable target references.
3. C: private local mailbox, replies and operator-contact flow.

## Completion evidence

- Only authorized mailbox participants read its ordinary endpoints.
- An edited post keeps a resolvable reported revision.
- Local posts/mail work without peers; local-only records are never offered to later federation adapters.

## Decisions before completion

Choose board creation privileges, mail size limits and Markdown subset; propose text-first messages until attachment handling has a separate design.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). A passing fixture is evidence for this module, not approval of new product scope. See [coverage](../COVERAGE.md) when changing source IDs.
