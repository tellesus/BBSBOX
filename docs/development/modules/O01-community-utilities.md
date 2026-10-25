# O01 — Search, chat and personal controls

Version 0.1.0 · 2026-10-25 · Tier 2 proposal · Tier 1 baseline 0.4.0.

**State:** Optional candidate; product approval required. No implementation or passed checks claimed.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C01](../contracts/C01-storage-runtime.md), [C05](../contracts/C05-private-content.md).
Look up only decision rows T09 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** P03, R03, R13, R17, R18. **Target gate:** G5. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M06](../modules/M06-boards-mail.md), [M08](../modules/M08-friends-events.md), [M13](../modules/M13-encrypted-forums.md).

## Product outcome and boundaries

Conditional P03 module; approve individual utilities independently. Covers local search, chat, SysOp paging, notifications, presence/caller history, welcome/MOTD refinements, personal blocks, leaving groups and invitation permissions.

## Recommended implementation

Prefer SQLite FTS5 for permitted public/local content; private decrypted search stays on the trusted client. Use node-local WebSockets for chat/paging and a durable local notification inbox. Presence is optional short-lived session state, not a record of physical movement. Blocks/invitation preferences are local user policy. Leaving an encrypted forum invokes M13 membership tools. Voluntary plaintext report disclosure, if approved, must be a deliberate case attachment.

## Ownership and interfaces

Proposed crates/utilities/ with separate search/chat/notification/preferences submodules; owns indexes and delivery preferences, not canonical forum content.

Paths above are proposed implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: decide one utility and its visibility rules; start with search or notification inbox.
2. B: implement chat/paging separately if approved.
3. C: personal controls and optional presence/MOTD refinements as separate tasks.

## Completion evidence

- Search and notifications never reveal content excluded by audience/profile settings.
- A personal block does not claim to impose a network-wide ban.
- Disconnected clients do not appear online indefinitely; no internet push dependency.

## Decisions before completion

Choose each utility before implementation; no one approval for this brief implicitly enables all listed P03 possibilities.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). A passing fixture is evidence for this module, not approval of new product scope. See [coverage](../COVERAGE.md) when changing source IDs.
