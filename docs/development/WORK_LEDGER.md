# Development work ledger

Version 1.1.0 · 2026-10-25 · Locked roadmap; M00/M02 In progress, remaining modules Unstarted. Execution updated 2026-10-30.

Evidence log — not an independent source of scope authority. Current choices live in the canonical roadmap and user instructions.

Design review is complete and the first M00 prototype has real local implementation evidence recorded below. Approval does not move a module to Complete; only its required checks do.

| Module | Scope | Execution state | Evidence / commit | Next action |
| --- | --- | --- | --- | --- |
| [M00](modules/M00-foundation-spikes.md) | Approved | In progress | 2026-10-30: Rust/API/store and HTTPS browser checks pass; see run record below | G0-local passed; M02 B next, plus trusted-client study; phone checks at G1/G2 |
| [M01](modules/M01-offline-appliance.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [M02](modules/M02-storage-jobs.md) | Approved | In progress | A/B implemented; 16 local integration tests and finite concurrent load; see current record | C: blobs/budgets/snapshots |
| [M03](modules/M03-identity-access.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [M04](modules/M04-signed-events.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [M12](modules/M12-maintenance-recovery.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [M05](modules/M05-interface-themes.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [M06](modules/M06-boards-mail.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [M07](modules/M07-directory-profiles.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [M08](modules/M08-friends-events.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [M09](modules/M09-clans-cohorts.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [M10](modules/M10-local-files.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [M11](modules/M11-local-games.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [M13](modules/M13-encrypted-forums.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [M14](modules/M14-complaints-moderation.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [M15](modules/M15-selected-federation.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [M16](modules/M16-pigeons-lump-drops.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [M17](modules/M17-peer-trust.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [M18](modules/M18-game-passports.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [M19](modules/M19-neighborhood-pilot.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [O01](modules/O01-community-utilities.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [O02](modules/O02-post-office.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [O03](modules/O03-neighborhood-exchange.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [O04](modules/O04-community-newspaper.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [O05](modules/O05-local-wiki.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [O06](modules/O06-meshcasts.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [O07](modules/O07-mesh-atlas.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [O08](modules/O08-relay-profile.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [O09](modules/O09-guest-kiosk.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [O10](modules/O10-game-experiments.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [O11](modules/O11-business-rewards.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [O12](modules/O12-recognition-extras.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |

## Handoff record

Append date; module/slice; roadmap/product baseline; optional rationale ADR; contract/schema revisions; source commit; test commands or observation records; actual results; limitations; migration/rollback impact; next bounded slice.

Investigating → Ready → Implementing → Validating → Complete tracks execution. A failed feasibility check blocks only dependent claims and is recorded with a next action. A passing mock is not real integration.

## Release capability record

Record artifact/signature, matching source/license materials, supported hardware/devices, installed module versions, actual AC results, pending approved capabilities, operator runbook and restore horizon. G4 is the core pilot. Full accepted optional scope completes only at G5.

## 2026-10-30 — M00 A / desktop B foundation experiment

Authority: user instruction “go ahead and build”; roadmap 1.0.1 / product 0.5.1; C01/C02/C05 v1.0.0; sources R03/R04/R05/R13/R15/R17/R28 and P04/P06/P07/P09/P21. Implementation: [spikes/m00 runbook](../../spikes/m00/README.md); commit is this record's containing Git revision. Product scope and shared contracts are unchanged.

Built a real static SvelteKit page and Rust/Axum HTTPS node, SQLite WAL/FULL persistence, deterministic detached-body CBOR/COSE_Sign1 Ed25519 signing and SHA-256 event IDs. The public messagebook uses the node's persistent fixture key and display labels; no person/account proof is implied. Read verification rejects corruption, an unchanged request ID retries idempotently, changed retry contents fail, and a cold process restart preserves the fixture key and history. This schema uses bbsbox.m00.event/v0, not a frozen protocol.

Test host: Windows x64 10.0.26300; AMD Ryzen 7 7800X3D, 16 logical CPUs, 63 GiB reported memory. This workstation is not a claimed low-end reference appliance. Rust 1.99.1 x86_64-pc-windows-gnullvm with portable LLVM 23.1.4; Node 24.22.1; OpenSSL 3.0.13; Edge 156 via Playwright 1.62.1. Build tools are project-local or pre-existing; system PATH/trust stores were not changed.

| Check | Result |
| --- | --- |
| Rust tests: cargo test --locked --offline --manifest-path node/Cargo.toml | 4 passed: signed/body/key/scope/chain checks; HTTP/1 + HTTP/2 origin handling; API validation; persistence/retry/corruption. |
| Rust formatting and clippy --all-targets -- -D warnings | Passed. |
| Static SvelteKit/TypeScript check and build | Passed, zero diagnostics. Assets load from the local node. |
| Actual HTTPS browser journey | 4 passed: CA trust/rejection; posting/reload plus independent COSE verification using Node crypto/cbor-x; literal HTML text; narrow layout and failed-send draft; concurrent retry deduplication and cold process restart. |
| Application network observation | Zero non-node requests during the routed browser journey; runtime has no outbound HTTP client. Physical WAN-absent/OS-wide egress measurement is pending. |
| TLS handling | Fresh local CA/leaf generated; no global trust enrollment. Browser test pins only the leaf SPKI in its temporary process; Node separately checks CA validation and rejects unknown trust. This is not proof of phone trust enrollment. |
| Optimized Windows build | 5,985,792-byte executable; a single idle process snapshot showed 8,306,688 bytes working set / 2,105,344 bytes private memory. Compiler/runtime DLLs and web assets are additional. This is a workstation observation, not a minimum hardware or load-capacity claim. |

The browser tests found and verified a correction for missing Host headers in HTTP/2: origin checks now read URI authority when needed. A narrow-layout decoration overlap was corrected after visual inspection. Test certificates, fixture keys, databases, build tools and browser state are ignored and excluded from publication. CI repeats the test recipe on Ubuntu; its actual result is tracked separately from local evidence.

Still pending: actual Debian/repurposed hardware, Android Chrome/iOS Safari, fresh local-address/trust enrollment, independently signed Tauri installation/update trust, OpenMLS stale writers/removal/epochs/concurrent governance, real recovery and reference-hardware budgets. M00 stays In progress; G0-local and G2 are not passed. Next bounded slice is physical-device B plus the independent C experiments. Other modules remain Unstarted.

## 2026-10-30 — M00 C/D local cryptography and runtime-isolation experiments

Authority: U22 continued local development/testing with phone checks at major milestones. Baseline: roadmap 1.1.0/product 0.6.0; C02 v1.0.0 / C05 v1.1.0. Sources R04/R13/R15/R17/R28, P04/P07/P09 and AC01/AC11/AC14/AC15. Code and detailed findings: [MLS experiment](../../spikes/m00/mls/README.md), network script/run recipe in the [M00 runbook](../../spikes/m00/README.md). The original prototype's [Ubuntu CI run](https://github.com/tellesus/BBSBOX/actions/runs/41141117156) passed; full device/appliance support was not inferred.

Eight real OpenMLS tests pass locally on Windows gnullvm with Rust 1.99.1, OpenMLS 0.9.2 and provider/credential/traits 0.6.0. Each actor has separate credentials, group state and crypto storage. Covered: disconnected/reordered writers and replay/tamper denial; missing-commit catch-up; stale removal/current-epoch exclusion/fresh join without history; 32/33-update retention boundary; competing valid branches and missing app-level owner enforcement; encrypted snapshots with fresh-provider reload/wrong-key/tamper denial; backup rollback losing later replay state; receive-only restore followed by fresh owner-authorized join while other members keep working. Format/clippy checks pass. The recovery guard is a harness implementation; no private client or admission layer is claimed complete.

Six local Python tests pass for the syscall auditor: real listener/accepted traffic is required, empty/partial traces cannot pass, failed IPv4/IPv6 connect attempts and explicit datagram/message destinations are rejected. The actual node-in-namespace experiment runs on Linux CI with only loopback and no IPv4/IPv6 gateway; it exercises HTTPS, compiled assets, a signed message and duplicate retry for both IP families. Its report/trace artifacts and actual outcome are recorded separately; a parser unit test alone is not isolated-runtime evidence.

C05 now explicitly requires owner-authorized control proofs bound to forum/parent epoch/commit hash and receive-only backup restoration until fresh authorized membership state. Retention and backup replay behavior are measured limits, not unconditional delivery or recovery guarantees. M13/M15/M16 own actual authority/transport integration. Native Tauri distribution remains unverified. Device checks are scheduled for G1/G2; M00 is In progress and G2 remains open. Pending reference Debian hardware, actual router/DNS/firewall provisioning and phones remain distinct from the namespace test.

## 2026-10-30 — G0-local evidence and M02 A storage foundation

The [Linux experiment run](https://github.com/tellesus/BBSBOX/actions/runs/41148611023) for commit 63bd54c passed both jobs: the actual IPv4/IPv6 node namespace smoke test, six syscall-auditor tests, original node/browser checks and all eight OpenMLS fixtures. The workflow requires only-loopback/no-gateway state and rejects attempted outbound network syscalls, not just successful egress. Its JSON/trace artifacts retain the bounded evidence. **G0-local is passed for continuing local development.** M00 remains In progress for trusted native-client/distribution and milestone device evidence; G1/G2 have not passed.

Authority U22; roadmap 1.1.0/product 0.6.0; C01 v1.1.0. M02 A is implemented at [crates/storage](../../crates/storage/README.md): an owned blocking WAL/FULL writer, atomic scoped command fingerprints/receipts, ordered migrations with SQL hashes, schema/foreign-database guards and independent read-only connections. The root Cargo workspace excludes disposable M00 crates; all toolchain pins remain Rust 1.99.1. Namespaces bind actor/action/resource; modules own request canonicalization and do not manage nested transaction boundaries.

Six local integration tests pass on Windows gnullvm: durable retry after reopen; changed ID/request rejection and distinct scopes; failed-work rollback with an independent reader progressing; migration failure/changed or removed migration/downgrade rejection; foreign DB and registry-gap rejection; oversized receipt and exact SQLite disk-full rollback; process exits before and after durable commit followed by one-effect retry. The disk-full test uses SQLite's page limit, not host disk exhaustion. Rust format and clippy checks pass. Linux CI repeats this workspace separately; its result must be checked for the implementation commit.

M02 A supplies primitives only. The bounded writer actor, priority/fairness and durable job claims belong to B; blobs, reserved disk budgets, consistent snapshot hooks and measured import capacity belong to C. The public prototype is not migrated or wired to this library. M02 and full AC07/AC15 remain In progress/pending. Next bounded slice: M02 B; phones remain scheduled for G1/G2.

## 2026-10-30 — direct interface copy

User requested removal of motivational/promotional wording from the site. Replaced the M00 hero, navigation, form/empty-state copy, callout and footer with functional labels and status text; removed the decorative slogan stamp. M05 brief revision 1.1.1 records the writing direction for future interface work. Updated existing browser journeys and the isolated-runtime page check to the current labels. Feature scope and roadmap/product baselines are unchanged.

## 2026-10-30 — M02 B bounded writer and transactional jobs

Authority: continued autonomous local development under U22 and current user instruction. Roadmap 1.1.0/product 0.6.0; C01 v1.2.0; M02 brief revision 1.1.1. Sources R05/R07/R12, P04/P06, AC01/AC07/AC15; no new product scope. Implementation is this record's containing commit. Defaults are 64 pending tasks, one executing task and an eight-interactive/one-background service ratio while both queues are pending. Async replies avoid running SQLite on request threads; accepted work survives caller disconnection. Jobs bind immutable queue/key arguments, fence stale tokens before/after completion, persist retries/pauses and recover interrupted claims only under the exclusive writer lock. Wall-clock schedules are distinct from monotonic leases. Core schema 2 upgrades schema 1 atomically.

Six A plus ten B integration tests pass on the recorded Windows workstation, including actual child-process exits before/after job completion followed by recovery to one effect. Formatting/clippy pass. The optimized finite workload concurrently completed 400 interactive writes and 200 jobs importing 20,000 rows (100/job) in 463 ms; interactive commit latency p50 19.342 ms, p95 24.054 ms, maximum 24.585 ms. This is an observation, not an appliance capacity promise. CI repeats the checks and saves its own workload result. M02 C, actual HTTP/module integration, M12 coordinated backups and full AC07/AC15 remain pending. Next: C, content-addressed blobs, disk reserve and core snapshot/restore hooks.
