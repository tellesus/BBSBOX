# M02 — Durable storage and bounded jobs

Version 0.1.0 · 2026-10-25 · Tier 2 proposal · Tier 1 baseline 0.4.0.

**State:** Planned capability; implementation choices proposed. No implementation or passed checks claimed.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C01](../contracts/C01-storage-runtime.md).
Look up only decision rows T02, T14 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R05, R07, R12, P04, P06, AC01, AC07, AC15. **Target gate:** G1. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M00](../modules/M00-foundation-spikes.md).

## Product outcome and boundaries

Provide crash-safe storage, deduplicated commands and background work shared by feature modules. Imports must leave ordinary local interaction usable.

## Recommended implementation

Use SQLite WAL through rusqlite with a dedicated bounded writer and read connections. Explicit SQL migrations and transactional outbox/jobs are preferable to a second queue service. Module-owned tables share one transaction boundary when receipt/credit or command/state changes must commit together. Files use content-addressed local storage.

## Ownership and interfaces

Proposed crates/storage/; migrations registry, jobs, command receipts, blob metadata and consistent snapshot hooks. Feature rows remain owned by their feature module.

Paths above are proposed implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: transaction/command idempotency and migration harness.
2. B: durable worker claims/retries and prioritized bounded queues.
3. C: blob lifecycle, disk limits and snapshot hooks.

## Completion evidence

- Kill a worker between validation and commit; a retry applies the command once.
- Measure local requests during a sustained import, and verify background work still makes progress.
- Disk-full and interrupted migration leave a recoverable, intelligible state.

## Decisions before completion

Select initial queue budgets and storage limits from M00 measurements; specify their tuning and persistence.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). A passing fixture is evidence for this module, not approval of new product scope. See [coverage](../COVERAGE.md) when changing source IDs.
