# M02 — Durable storage and bounded jobs

Version 1.1.0 · 2026-10-25 · Locked development baseline · Product baseline 0.6.0.

**State:** Approved for implementation under U19; unstarted. Runtime and hardware evidence remains pending.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C01](../contracts/C01-storage-runtime.md).
Look up only decision rows T02, T14 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R05, R07, R12, P04, P06, AC01, AC07, AC15. **Target gate:** G1. Scope summaries below preserve these source IDs; reconcile conflicts with the current product requirements and user instructions.

**Prerequisites:** [M00](M00-foundation-spikes.md).

## Product outcome and boundaries

Provide crash-safe storage, deduplicated commands and background work shared by feature modules. Imports must leave ordinary local interaction usable.

## Locked implementation

Use SQLite WAL through rusqlite with a dedicated bounded writer and read connections. Explicit SQL migrations and transactional outbox/jobs are preferable to a second queue service. Module-owned tables share one transaction boundary when receipt/credit or command/state changes must commit together. Files use content-addressed local storage.

## Ownership and interfaces

Planned crates/storage/; migrations registry, jobs, command receipts, blob metadata and consistent snapshot hooks. Feature rows remain owned by their feature module.

Paths above are planned implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: transaction/command idempotency and migration harness.
2. B: durable worker claims/retries and prioritized bounded queues.
3. C: blob lifecycle, disk limits and snapshot hooks.

## Completion evidence

- Kill a worker between validation and commit; a retry applies the command once.
- Measure local requests during a sustained import, and verify background work still makes progress.
- Disk-full and interrupted migration leave a recoverable, intelligible state.

## Locked defaults and verification

SQLite WAL with synchronous=FULL, one bounded writer, transactional outbox and explicit migrations are selected. Start imports at 100 events per transaction and yield between batches; make budgets configurable and measure fairness. Reject writes cleanly at a configurable reserved free-space threshold; do not delete canonical history to manufacture space. M00 measures defaults before a capacity promise. Snapshot/worker participation follows C01.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
