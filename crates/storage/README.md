# M02 A — transaction and migration foundation

Implementation reference for the [canonical storage brief](../../docs/development/modules/M02-storage-jobs.md) and [C01](../../docs/development/contracts/C01-storage-runtime.md). Product authority remains the roadmap and user instructions.

The first core crate owns a SQLite WAL/FULL writer connection. `Database::transaction` records a scoped command's request fingerprint, module mutation and response receipt in one transaction. A matching retry returns the durable response without rerunning work; reusing an ID with different request bytes fails. Namespaces must bind the authenticated actor/action/resource context, and callers own the canonical request encoding. There is no HTTP authentication or game database access in this crate.

The migration registry validates contiguous, unique module versions and records SQL hashes. All missing migrations run in one transaction; failing SQL rolls back the module/core bootstrap. Altered or removed applied migrations, schema downgrades and foreign databases fail explicitly. Keep migrations registered when a module is disabled. The reader helper opens an independent read-only connection; tests verify committed state stays readable during a write.

## Run

From the repository root with Rust 1.99.1 and a native C compiler:

```sh
cargo test --locked --offline -p bbsbox-storage
cargo clippy --locked --offline --workspace --all-targets -- -D warnings
```

Fetch locked dependencies once before using `--offline`. This workspace's portable Windows toolchain is available through `. ./spikes/m00/tools/local-env.ps1`; it changes only that shell. The root workspace excludes the disposable M00 experiments so their existing lockfiles/run paths remain independent.

Six integration tests cover scoped idempotence after reopening, rejected ID reuse, state/receipt rollback, independent WAL reads, migration failure/change/downgrade rejection, foreign databases, oversized receipts, SQLite disk-full rollback and process exit immediately before/after command commit. SQLite's page limit simulates disk-full without filling the host disk. Retry after either crash gives one committed effect.

## Remaining M02 work

This owned blocking connection is slice A, ready for the dedicated writer service. It is not yet wired to HTTP, a bounded asynchronous queue or the public prototype. Slice B adds the writer actor, fairness, durable job claims/retries and progress. Slice C adds blobs, reserved disk budget, consistent snapshots and retention hooks. Production import fairness/capacity and full crash/migration recovery acceptance remain open. Prototype databases use a separate disposable schema and are deliberately not opened as core storage.
