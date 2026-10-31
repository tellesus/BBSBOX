# M02 A/B — storage and durable jobs

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

## Writer and jobs (B)

`writer::Writer::start` holds an OS file lock and owns the blocking connection on one thread. Node modules submit through cloned handles; direct `Database` opens are bootstrap/test primitives and do not acquire that lock. Async callers await `Pending::wait`; synchronous tools use `blocking_wait`. Shutdown closes admission and drains accepted tasks. Disconnection does not cancel accepted work. A panic returns `WorkPanicked`; consult durable receipts before retrying. Recursive submission is rejected.

Defaults: 64 pending tasks plus one executing task; eight interactive tasks then one background task while both are pending. Saturation immediately returns `QueueFull`. Bound individual commands/batches too; queue length is not a hard heap limit.

`jobs::enqueue(tx, JobSpec)` inserts an outbox item in the caller's transaction. A queue/key binds immutable kind, payload and initial schedule. Payload/receipt limits are 1 MiB. Claims select enabled kinds, increment attempts and issue private random tokens. Completion checks the lease before/after work and commits effects plus response together; repeated completion returns that response. Expired/replaced claims cannot commit. Leases use monotonic time; retry dates use wall time. `Retry::Manual` pauses persistently until resumed. Startup recovers unfinished jobs only after obtaining the writer lock; paused/done jobs stay intact. External side effects need durable idempotence/outbox handling; claims alone do not provide it.

Ten additional integration tests cover bounded/fair admission, a live single-thread async executor, panic rollback, stale/replaced/overlong claims, retry/pause, exclusive startup, schema-1 upgrade and actual process exits before/after job completion. Core schema 2 upgrades schema 1 atomically and retains receipts. Old code rejects schema 2; pre-update backup orchestration follows in C/M12 before deployment.

`cargo run --release --locked --offline --example writer_load` concurrently performs 400 interactive writes and 200 background jobs of 100 rows, verifies all 20,000 rows and prints latency observations. This finite workload is not a reference-hardware capacity claim.

## Remaining M02 work

C adds blobs, reserved disk budgets, consistent snapshot/retention hooks and a real restore. The crate is not wired to HTTP or the public prototype. Prototype databases deliberately keep a separate disposable schema. M02 remains In progress.
