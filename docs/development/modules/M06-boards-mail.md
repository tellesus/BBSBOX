# M06 — Local boards and private mail

Version 1.1.0 · 2026-10-25 · Locked development baseline · Product baseline 0.6.0.

**State:** Approved for implementation under U19; unstarted. Runtime and hardware evidence remains pending.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C02](../contracts/C02-identity-events.md), [C05](../contracts/C05-private-content.md).
Look up only decision rows T05, T09 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R07, R12, R13, P03, AC01, AC02, AC07. **Target gate:** G1. Scope summaries below preserve these source IDs; reconcile conflicts with the current product requirements and user instructions.

**Prerequisites:** [M04](M04-signed-events.md), [M05](M05-interface-themes.md).

## Product outcome and boundaries

Deliver local threaded public discussion and recipient-restricted local mail, including operator contact. This module does not claim host-blind mail or private encrypted forums; M13 and optional O02 supply those distinct capabilities.

## Locked implementation

Use immutable post/message events and indexed thread/mailbox projections. Author edits reference immutable revisions; tombstones preserve causal ordering. Render a bounded Markdown subset with sanitized output and no executable embedded content. Local mail uses recipient access controls; label its host-visible storage model accurately. File attachments are excluded from the first slice pending an explicit attachment decision, while local file areas remain available via M10.

## Ownership and interfaces

Planned crates/discussions/ and crates/mail/; thread/post revisions, mailboxes, reply links, recipient policies and operator contacts. Both expose stable post/message references to complaints and sync.

Paths above are planned implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: create/read/reply boards with explicit local-only distribution.
2. B: edit/withdraw/replay behavior and stable target references.
3. C: private local mailbox, replies and operator-contact flow.

## Completion evidence

- Only authorized mailbox participants read its ordinary endpoints.
- An edited post keeps a resolvable reported revision.
- Local posts/mail work without peers; local-only records are never offered to later federation adapters.

## Locked defaults and verification

Members may post/reply on SysOp-created boards; delegated board creation is configurable. Local mail is recipient-restricted and explicitly host-visible in this first module. Text-first messages use a sanitized Markdown subset with links and no remote embeds/active HTML; suggested initial text cap is 64 KiB, bounded by M04 limits. File-library links remain local. Message attachments are outside this baseline rather than an unresolved prerequisite.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
