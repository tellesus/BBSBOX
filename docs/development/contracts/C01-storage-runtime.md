# C01 — Storage, runtime and module boundary

Version 1.2.0 · Accepted for implementation · Owner M02; operations co-owner M12.

## Responsibilities and interface

Use one Rust node service with explicit internal modules and one local SQLite database using WAL. Modules own prefixed tables and ordered migrations; only the owning service writes those tables. Cross-module operations call typed services within a shared transaction when atomicity matters. Read projections are replaceable; signed records, receipts, role decisions, and user data are durable. Large local files are content-addressed on disk with database metadata. This is not database-file replication.

Selected interfaces: `transaction(namespace, command_id, canonical_request, work)`, `enqueue(queue, idempotency_key, payload)`, `claim_job/finish_job`, `module_query`, `blob_put/get`, `snapshot_manifest`. These are design names, not a published API. The command namespace binds authenticated actor/action/resource context; canonical request bytes bind all relevant arguments. Reusing an ID with changed bytes fails. Mutation and durable response receipt commit together. Browser endpoints use versioned JSON schemas/OpenAPI; core Rust interfaces do not expose database handles to games.

Use a bounded writer worker with rusqlite off async request threads. Keep SQLite on a local filesystem. Default durable transactions use FULL synchronous behavior pending M00 measurements. HTTP success for a mutation follows durable commit; filesystem bytes must be finalized before their row becomes visible. Recover orphan temporary files safely after interruption.

## Jobs and recovery

Serve local interactive work ahead of incoming bulk processing. Give background queues bounded batches plus a minimum service share so sync cannot starve indefinitely. Retry with persistent progress; failures carry a typed reason and retry condition. A command ID deduplicates browser retries; a network event ID deduplicates synchronization. They are different identifiers.

The M02 writer defaults to 64 pending tasks plus one executing task; while both priorities are pending, serve at most eight interactive tasks before one background task. Reject saturation explicitly. Accepted work survives caller disconnection. Keep callbacks bounded and never submit recursively. A process lock protects startup job recovery; direct database opens are bootstrap/test interfaces. Jobs use fresh claim tokens, monotonic lease deadlines and fenced transactional completion; calendar retry times use wall time. Restart makes unfinished claims retryable, preserving paused/done jobs. External effects require their own durable idempotence/outbox; a claim alone is insufficient.

Take a consistent SQLite backup through its backup API plus a manifest of referenced blobs, configuration, node secrets and counters. Protect required blobs from deletion while taking the snapshot. Client private keys are a separate client export; a server backup does not magically contain them. Do not copy only the main SQLite file while ignoring live WAL state.

## Module lifecycle

First-party features are compile-time Rust modules enabled by local configuration. Every module declares ID, schema version, dependencies, routes, migrations, job types and owned state. Disabling a module hides its UI/stops its new jobs while retaining data; uninstall/export/erase is separate explicit work. Pending unknown module events stay unsupported, not falsely applied. Games use C04 instead of loading code into this process.

## Compatibility evidence

Migrations need pre-update snapshots, interrupted-upgrade recovery, and defined minimum readable schema. Rollback restores a matching app/data snapshot or uses a proven backward-compatible schema. An old snapshot is never activated alongside the same live signing streams: M03/M12 rotate writer credentials/incarnations before resuming. Measure concurrent local use plus imports, disk-full behavior and a real restore. Technology references: [SQLite WAL](https://www.sqlite.org/wal.html), [backup API](https://www.sqlite.org/backup.html), [rusqlite](https://docs.rs/rusqlite/latest/rusqlite/).


## Coordinated game snapshots and irreversible effects

Game workers own separate databases. The maintenance coordinator pauses new mutations, drains commands, asks every installed game for a durable checkpoint ID, then snapshots core/jobs/receipts and referenced game checkpoints under one backup manifest. Resume only after the manifest is durable. A worker that cannot checkpoint makes the backup incomplete; never label a core-only copy a complete node backup. Importers, jobs and games test replay after restoration together.

Within intact durable state, transaction IDs prevent duplicate effects. A restore cannot recover writes made after the last surviving snapshot by itself. Start restored writers in recovery mode, reconcile surviving signed events/receipts where available, record the known recovery horizon, then rotate signing incarnations. Where consumed coupons/manual rewards cannot be reconstructed, keep the affected issuance/redemption records disabled for local operator reconciliation; unrelated community service continues. Do not claim universal exactly-once external effects after loss of all evidence.

Reserve release/receipt/control metadata from ordinary content eviction. Retention never silently deletes an unacknowledged export or required dependency; surface pressure and let the operator choose explicit policy. Update verification needs an anti-rollback version counter and a recorded recovery override for deliberate compatible rollback; release-key rotation is authorized by the previous trusted release key and included offline.
