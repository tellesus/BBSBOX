# M02 — storage, jobs and core snapshots

Implementation reference for the [canonical storage brief](../../docs/development/modules/M02-storage-jobs.md) and [C01](../../docs/development/contracts/C01-storage-runtime.md). Product authority remains the roadmap and user instructions.

The first core crate owns a SQLite WAL/FULL writer connection. `Database::transaction` records a scoped command's request fingerprint, module mutation and response receipt in one transaction. A matching retry returns the durable response without rerunning work; reusing an ID with different request bytes fails. Namespaces must bind the authenticated actor/action/resource context, and callers own the canonical request encoding. There is no HTTP authentication or game database access in this crate.

The migration registry validates contiguous, unique module versions and records SQL hashes. All missing migrations run in one transaction; failing SQL rolls back the module/core bootstrap. Altered or removed applied migrations, schema downgrades and foreign databases fail explicitly. Keep migrations registered when a module is disabled. The reader helper opens an independent read-only connection; tests verify committed state stays readable during a write.

Consumers with typed business errors use `transaction_service` and `Handle::submit_service`; storage/panic errors convert to that module's error type. `Database::write` supplies a committed service transaction without a durable response receipt for operations such as login. Password/session secrets and low-entropy password fingerprints never belong in general command receipts. Existing command/queue APIs retain their storage-error defaults.

## Run

From the repository root with Rust 1.99.1 and a native C compiler:

```sh
cargo test --locked --offline -p bbsbox-storage
cargo clippy --locked --offline --workspace --all-targets -- -D warnings
```

Fetch locked dependencies once before using `--offline`. This workspace's portable Windows toolchain is available through `. ./spikes/m00/tools/local-env.ps1`; it changes only that shell. The root workspace excludes the disposable M00 experiments so their existing lockfiles/run paths remain independent.

Six integration tests cover scoped idempotence after reopening, rejected ID reuse, state/receipt rollback, independent WAL reads, migration failure/change/downgrade rejection, foreign databases, oversized receipts, SQLite disk-full rollback and process exit immediately before/after command commit. SQLite's page limit simulates disk-full without filling the host disk. Retry after either crash gives one committed effect.

## Writer and jobs (B)

`writer::Writer::start` holds an OS file lock and owns the blocking connection on one thread. Node modules submit through cloned handles; direct `Database` opens are bootstrap/test primitives and do not acquire that lock. Async callers await `Pending::wait`; synchronous tools use `blocking_wait`. Shutdown closes admission and drains accepted tasks. Disconnection does not cancel accepted work. A panic returns `WorkPanicked`; consult durable receipts before retrying. Recursive submission is rejected.

Defaults: 64 pending tasks plus one executing task; eight interactive tasks then one background task while both are pending. Saturation immediately returns `QueueFull`. Bound individual commands/batches too; queue length is not a hard heap limit.

`jobs::enqueue(tx, JobSpec)` inserts an outbox item in the caller's transaction. A queue/key binds immutable kind, payload and initial schedule. Payload/receipt limits are 1 MiB. Claims select enabled kinds, increment attempts and issue private random tokens. Completion checks the lease before/after work and commits effects plus response together; repeated completion returns that response. Expired/replaced claims cannot commit. Leases use monotonic time; retry dates use wall time. `Retry::Manual` pauses persistently until resumed. Startup recovers unfinished jobs only after obtaining the writer lock; paused/done jobs stay intact. External side effects need durable idempotence/outbox handling; claims alone do not provide it.

Ten additional integration tests cover bounded/fair admission, a live single-thread async executor, panic rollback, stale/replaced/overlong claims, retry/pause, exclusive startup and actual process exits before/after job completion. Core schema 3 atomically upgrades schema 1/2 and retains receipts/jobs; older binaries reject newer schemas. M12 coordinates pre-update snapshots and matching app/data rollback before deployment.

`cargo run --release --locked --offline --example writer_load` concurrently performs 400 interactive writes and 200 background jobs of 100 rows, verifies all 20,000 rows and prints latency observations. This finite workload is not a reference-hardware capacity claim.

## Blobs, disk pressure and snapshots (C)

`put_blob(namespace,key,Read)` streams up to the configured maximum, hashes bytes with SHA-256, flushes a temporary file and finalizes it without overwriting an existing file before committing metadata and a protecting reference. Source readers must be bounded local staged files; request handlers must not feed a network stream to the writer. Blob roots use the canonical database path, including when opened through an alias. Reads verify size/hash. Uploads retain a reference until a module transaction consumes it. `set_reference`/`remove_reference` operate in that transaction, alongside module state/outbox work. Every pending export or dependency must retain a reference; pressure never evicts it. `collect_blobs` deletes only unreferenced rows before deleting bytes; a crash can leave safe orphans. `recover_blob_orphans` removes managed staging/unregistered content files under writer ownership. Both limit deletions to 1–1024 per call; orphan scanning itself is not constant-time. Unknown subdirectories are never recursively erased.

`writer::Config.disk` selects admission floors (ordinary content 256 MiB; control 32 MiB) and maximum blob size (64 MiB). `transaction_with_budget` lets trusted services choose a class and conservative growth estimate including WAL/receipt space. Ordinary `transaction` estimates 64 KiB; large operations supply a better estimate. Jobs use control admission; enqueue large content work in the originating content transaction. Pre/post checks reject pressure and roll back writes; matching durable receipts remain readable under pressure. These floors reserve application headroom, not disk blocks against other processes. SQLite/filesystem errors still need handling. `start_with_resources` injects a space probe for controlled tests; local fixtures never fill the host disk.

`snapshot_core` is a maintenance operation after admission is paused/drained. Writer serialization protects the inventory from collection. It uses SQLite's backup API, closes a standalone database, copies/verifies inventoried blobs and writes a bounded manifest last (format `bbsbox.core-snapshot/v1`, schema 3, at most 50,000 blobs/8 MiB manifest). Partial snapshots cannot verify. Existing destinations are never overwritten. `verify_core` checks database integrity, references and every hash. `restore_core` verifies copies into a new directory and leaves `node.sqlite.recovery-required`; normal writer startup refuses activation. Database-level readers/tests can inspect it. M03/M12 own recovery-horizon reconciliation, credential rotation and controlled activation. Configuration, external secrets, client keys and game checkpoints are not supplied by this core-only hook; it is not a complete-node backup. Linux directories/files are private (0700/0600) and parent directories are synced. Windows evidence covers process crashes; appliance power-loss behavior remains M01/M12 evidence.

Eight C tests plus a process-killed migration test bring Windows coverage to 25 integration tests. Linux adds an alias/private-permission check. Fixtures include failed metadata after file finalization, interrupted streams, corrupt bytes/manifests, reference-aware collection, reserved control room, schema-2 recovery, live-WAL backup and a real restore preserving receipts/jobs/blobs. Filesystem finalization tests need normal Windows permissions; the Codex restricted runner denies temporary-file rename, while the normal-permission suite passes.

## Integration boundary

This crate is not wired to HTTP or the public prototype; consumers own real integration. Prototype databases retain their separate disposable schema. Actual appliance capacity, full-node coordinated backups and release acceptance remain with M01/M10/M12/M19.
