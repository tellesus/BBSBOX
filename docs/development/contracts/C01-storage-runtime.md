# C01 — Storage, runtime and module boundary

Version 0.1.0 · Proposed · Owner M02; operations co-owner M12.

## Responsibilities and interface

Prefer one Rust node service with explicit internal modules and one local SQLite database using WAL. Modules own prefixed tables and ordered migrations; only the owning service writes those tables. Cross-module operations call typed services within a shared transaction when atomicity matters. Read projections are replaceable; signed records, receipts, role decisions, and user data are durable. Large local files are content-addressed on disk with database metadata. This is not database-file replication.

Proposed interfaces: `transaction(command_id, work)`, `enqueue(queue, idempotency_key, payload)`, `claim_job/finish_job`, `module_query`, `blob_put/get`, `snapshot_manifest`. These are design names, not a published API. Browser endpoints use versioned JSON schemas/OpenAPI; core Rust interfaces do not expose database handles to games.

Use a bounded writer worker with rusqlite off async request threads. Keep SQLite on a local filesystem. Default durable transactions use FULL synchronous behavior pending M00 measurements. HTTP success for a mutation follows durable commit; filesystem bytes must be finalized before their row becomes visible. Recover orphan temporary files safely after interruption.

## Jobs and recovery

Serve local interactive work ahead of incoming bulk processing. Give background queues bounded batches plus a minimum service share so sync cannot starve indefinitely. Retry with persistent progress; failures carry a typed reason and retry condition. A command ID deduplicates browser retries; a network event ID deduplicates synchronization. They are different identifiers.

Take a consistent SQLite backup through its backup API plus a manifest of referenced blobs, configuration, node secrets and counters. Protect required blobs from deletion while taking the snapshot. Client private keys are a separate client export; a server backup does not magically contain them. Do not copy only the main SQLite file while ignoring live WAL state.

## Module lifecycle

First-party features are compile-time Rust modules enabled by local configuration. Every module declares ID, schema version, dependencies, routes, migrations, job types and owned state. Disabling a module hides its UI/stops its new jobs while retaining data; uninstall/export/erase is separate explicit work. Pending unknown module events stay unsupported, not falsely applied. Games use C04 instead of loading code into this process.

## Compatibility evidence

Migrations need pre-update snapshots, interrupted-upgrade recovery, and defined minimum readable schema. Rollback restores a matching app/data snapshot or uses a proven backward-compatible schema. An old snapshot is never activated alongside the same live signing streams: M03/M12 rotate writer credentials/incarnations before resuming. Measure concurrent local use plus imports, disk-full behavior and a real restore. Technology references: [SQLite WAL](https://www.sqlite.org/wal.html), [backup API](https://www.sqlite.org/backup.html), [rusqlite](https://docs.rs/rusqlite/latest/rusqlite/).
