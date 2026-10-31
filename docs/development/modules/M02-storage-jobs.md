# M02 — Durable storage and bounded jobs

Version 1.1.0 · 2026-10-25 · Locked development baseline · Product baseline 0.6.0.

**State:** In progress under U22 after G0-local evidence. Slice A has six passing local integration tests; B/C and full G1 evidence remain pending.

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

crates/storage/ now implements slice A: an owned SQLite writer, migration registry, scoped command fingerprints/durable receipts and independent read-only connections. Jobs, blobs and snapshot hooks follow in B/C. Feature rows remain owned by their module. See the [implementation runbook](../../../crates/storage/README.md).

The library is not wired to HTTP or the prototype. The bounded writer actor and fairness are B work; callers must not put the current blocking writer on async request threads. Module SQL uses the existing transaction rather than managing its own. Consume C01 and update affected consumers together.

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

Current local evidence: six tests pass for reopen/idempotency, changed request rejection, state/receipt rollback, WAL reads during a write, migration failure/change/downgrade and foreign-database denial. SQLite page limits verify exact disk-full rollback. Child processes exit before/after commit; retries produce one effect. Full module completion still requires B/C and measured sustained-import fairness. This is progress within AC07/AC15, not completion of those criteria.

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
