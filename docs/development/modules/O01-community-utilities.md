# O01 — Search, chat and personal controls

Version 1.1.0 · 2026-10-25 · Locked development baseline · Product baseline 0.6.0.

**State:** Approved for implementation under U19; unstarted. Runtime and hardware evidence remains pending.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C01](../contracts/C01-storage-runtime.md), [C05](../contracts/C05-private-content.md).
Look up only decision rows T09 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R28, P03, R03, R13, R17, R18. **Target gate:** G5. **Expansion acceptance:** AC23. Scope summaries below preserve these source IDs; reconcile conflicts with the current product requirements and user instructions.

**Prerequisites:** [M06](M06-boards-mail.md), [M08](M08-friends-events.md), [M13](M13-encrypted-forums.md), [M14](M14-complaints-moderation.md).

## Product outcome and boundaries

Approved P03 module with independently implemented slices. Covers local search, chat, SysOp paging, notifications, presence/caller history, welcome/MOTD refinements, personal blocks, leaving groups and invitation permissions.

## Locked implementation

Use SQLite FTS5 for permitted public/local content; private decrypted search stays on the trusted client. Use node-local WebSockets for chat/paging and a durable local notification inbox. Presence is optional short-lived session state, not a record of physical movement. Blocks/invitation preferences are local user policy. Leaving an encrypted forum invokes M13 membership tools. Voluntary plaintext report disclosure, when its prerequisites pass, must be a deliberate case attachment.

## Ownership and interfaces

Planned crates/utilities/ with separate search/chat/notification/preferences submodules; owns indexes and delivery preferences, not canonical forum content.

Paths above are planned implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: implement search and the local notification inbox with their visibility rules.
2. B: implement chat/paging separately when its prerequisites pass.
3. C: personal controls and optional presence/MOTD refinements as separate tasks.

## Completion evidence

- Search and notifications never reveal content excluded by audience/profile settings.
- A personal block does not claim to impose a network-wide ban.
- Disconnected clients do not appear online indefinitely; no internet push dependency.

## Locked defaults and verification

All named P03 utilities are approved: local public FTS5 search, client-side private search, durable local notification inbox, WebSocket chat/paging, opt-in presence/caller history, invitation controls, blocks and group exit. Presence expires after 120 seconds without heartbeat; caller history is opt-in and defaults to seven-day local retention. M05 owns MOTD data/rendering, O01 owns scheduling; M14 owns voluntary disclosure. Build inbox/search first, chat next, then personal/presence controls.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
