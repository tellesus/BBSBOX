# Development work ledger

Version 1.1.0 · 2026-10-25 · Locked roadmap; M02/M03 providers Complete; M00/M04 In progress. Other modules Unstarted. Execution updated 2026-10-31.

Evidence log — not an independent source of scope authority. Current choices live in the canonical roadmap and user instructions.

Design review is complete and the first M00 prototype has real local implementation evidence recorded below. Approval does not move a module to Complete; only its required checks do.

| Module | Scope | Execution state | Evidence / commit | Next action |
| --- | --- | --- | --- | --- |
| [M00](modules/M00-foundation-spikes.md) | Approved | In progress | 2026-10-30: Rust/API/store and HTTPS browser checks pass; see run record below | G0-local passed; trusted-client study and phone checks at G1/G2 remain |
| [M01](modules/M01-offline-appliance.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [M02](modules/M02-storage-jobs.md) | Approved | Complete | A–C: 25 Windows/26 Linux integration tests; CI 41167864139 passed | Consumers own HTTP/module integration; M12 owns full-node backup activation |
| [M03](modules/M03-identity-access.md) | Approved | Complete | A–C: 68 Windows/70 Linux workspace checks and independent vectors; CI 41172025063 passed | Consumers integrate current v2/legacy v1 profiles; native/HTTP/MLS gates remain downstream |
| [M04](modules/M04-signed-events.md) | Approved | In progress | A: 14 local codec/authority tests plus independent exact-byte vector | Confirm A on Linux; B: persistence/staging/deduplication/forks |
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

## 2026-10-30 — M02 C blobs, disk admission and core restore

The [A/B checkpoint CI](https://github.com/tellesus/BBSBOX/actions/runs/41167112872), commit 1b94e5a, passed all three Ubuntu jobs, including 16 core tests and the finite load observation. Current authority remains continued local development under U22/current user instructions. Baselines stay roadmap 1.1.0/product 0.6.0; C01 v1.3.0/M02 brief 1.1.2 record compatible provider details. Sources R05/R07/R12, P04/P06, AC01/AC07/AC15.

C adds streamed SHA-256 content-addressed bytes finalized before metadata/reference commit; transactional references protect pending exports/dependencies. Collection requires explicit reference release. File/metadata failures leave recoverable orphans. Configurable content/control admission floors default to 256/32 MiB, maximum blob 64 MiB; matching receipts remain available under pressure. Checks account for expected growth and recheck before commit; external disk use can still cause I/O failure. Writer startup uses configured resources before job recovery. Canonical database paths keep alias access on one blob store.

Core snapshot hooks use SQLite backup, a standalone database and verified independent blob copies, with a manifest written last. Restore verifies hashes/schema/references into a fresh directory; a persistent marker blocks writer activation pending M03/M12 recovery. This is explicitly core-only, excluding configuration, external secrets, client keys and coordinated game checkpoints. Linux directory/file permissions are 0700/0600 and parent directories sync; Windows tests establish process-crash behavior, not appliance power-loss proof.

Windows normal-permission verification: 25 integration tests pass, format/clippy/roadmap validator pass. Added fixtures cover stream process death, finalized-file/failed-metadata cleanup, corrupt files/manifests, non-eviction of pending exports, disk-floor rollback/control headroom, schema-2 upgrade rollback and a real live-WAL restore preserving blobs/receipts/paused jobs. A separate process is killed after spilling uncommitted migration WAL frames; schema/rows roll back and reapplication succeeds. Linux additionally checks alias access and private snapshot permissions; its C result remains pending until CI. The restricted Windows tool runner denies temporary-file finalization; rerunning with normal permissions passes without changing the implementation.

With real disk probes enabled, the finite load took 1,641 ms on this workstation at normal permissions: p50 71.104 ms, p95 77.235 ms, maximum 79.618 ms for 400 concurrent interactive writes while 200 jobs import 20,000 rows. The restricted-runner observation was slower (4,175 ms/p95 192.421 ms); neither is a reference-appliance limit. M02 awaits C Linux confirmation. Next provider consumer: M03 A local accounts/roles; full HTTP integration, node backup activation, appliance budgets and AC07/AC15 remain downstream work.

## 2026-10-30 — M02 provider complete; M03 A local accounts

The [C checkpoint CI](https://github.com/tellesus/BBSBOX/actions/runs/41167864139), commit 0f14169, passed all three Ubuntu jobs. Storage has 25 Windows/26 Linux integration tests, including the Unix alias/private-permission fixture; concurrent-load, static/browser, MLS and isolated-runtime regressions also passed. **M02 is Complete as a provider.** HTTP/feature integration and appliance/full-node recovery/release evidence remain with consumers; no gate advances merely from this provider status.

Authority: current continued-development instruction/U22. Roadmap 1.1.0/product 0.6.0, C01 v1.3.0/C02 v1.1.0, M03 brief 1.1.1; sources R07/R13/R14/R17, P07/P22, AC01/AC11/AC12/AC14/AC15. M03 A implementation: [local account reference](../../crates/identity/README.md), identity migration 1, containing commit. There is no portable person/device proof or private-client implementation in A.

Implemented atomic local SysOp bootstrap, normalized unique accounts, live Member/SysOp permissions, last-active-SysOp protection, active/role changes and local password reset/change. Password work uses RustCrypto Argon2 0.6 on a bounded authentication worker before writer submission; the writer fences pending verification against credential changes. Tokens are random; persisted sessions contain hashes. Origin/CSRF guards, host-only secure cookie headers, logout, retry cooldown and idle/absolute expiry are implemented. A fresh authentication-service boot ends browser sessions; this is independent of portable credential recovery. A member's password change requires their own current-password proof. Login/password operations never store bearer secrets or low-entropy password request hashes in generic receipts. Typed storage/writer adapters preserve module errors and rollback without changing existing APIs.

Local Windows results: 14 account integration tests plus one hashing-admission unit test, and all 25 storage regressions pass (40 total); format/clippy/roadmap validator pass. Tests use real 19 MiB Argon2 hashes and SQLite. Covered account creation/normalization, salted credentials, unknown/disabled login, origin/CSRF/logout, role revocation, last SysOp, password-reset race, own-password proof, idle versus absolute expiry, wall-clock correction/restart, bounded attempts, corrupt/excessive hash parameters and typed receipt/service/panic recovery. Linux confirmation for A is pending until this commit's CI. HTTP routing and actual browser cookie enforcement belong to M05 integration; B/C and full M03 acceptance remain open. Next: B, independent public identifiers/root-authorized device credentials and introductions.

## 2026-10-30 — M03 B public credentials and client signing core

The [A checkpoint CI](https://github.com/tellesus/BBSBOX/actions/runs/41168885917), commit 68b811a, passed all three Ubuntu jobs. Current authority remains continued autonomous development; roadmap 1.1.0/product 0.6.0, C01 v1.3.0/C02 v1.2.0, M03 brief 1.1.2. Sources remain M03's existing IDs. B implementation is this record's containing commit: [public credential profile](../../crates/credentials/README.md), [client core](../../client/identity/README.md) and identity migration 2.

Implemented domain-separated typed public IDs, fingerprint/QR cards containing only public root material, root-signed bounded-purpose device credentials, fresh credential incarnations, matching expected-person/device activation and purpose-limited client signatures. The node's production dependency is the public verifier; client key holders are separate. Canonical COSE/CBOR reconstruction, strict Ed25519 verification, fixed size/version/purpose limits and a public byte vector define credential v1. The resolver persists verified delegations transactionally, re-verifies stored bytes/IDs and leaves missing credentials unresolved. It never creates/links an account or grants forum/game authority. Causal revocation and actual resource authorization remain M04/M13 integration, not a property of a signature alone.

Local Windows results: 52 workspace tests pass (25 storage, 15 account, six credential-profile, three client-delegation and three resolver/migration checks), plus the independent Node crypto/cbor-x vector check. Format/clippy/roadmap validator pass. Tests cover exact public bytes, role ID separation, root/device mismatch, fresh incarnations, invalid issuer/signature, weak/reused keys, extra/unprotected fields, nonminimal CBOR, unsupported versions/purposes, excessive/trailing data, stored corruption and real migration 1→2 preserving an account/receipt. Node independently reconstructs payload/signature bytes, hashes IDs, verifies/reproduces Ed25519 and rejects tampering. Libraries: coset 0.4.2/ciborium 0.2.2/ed25519-dalek 3.0.0; the M00 experiment retains its independent lockfile. CI now includes client-only changes and repeats the independent vector check.

M03 remains In progress. B Linux confirmation, C protected recovery/destination linking, signed stream/counter integration, actual HTTP/browser/native-client and device acceptance remain pending. Next bounded work: C's encrypted user-held recovery and fresh-device restore behavior; preserve the existing M00/C05 receive-only MLS restore rule when integrating later client state.

## 2026-10-31 — M03 C protected recovery, destination links and node signer lifecycle

The [B checkpoint CI](https://github.com/tellesus/BBSBOX/actions/runs/41169911897), commit 74d4edd, passed all three Ubuntu jobs, including the independent vector verifier. Authority remains continued development/U22; roadmap 1.1.0/product 0.6.0, C01 v1.3.0/C02 v1.3.0, M03 brief 1.1.3, identity migrations 3/4. The graph/index now explicitly identify M03's G0-local prerequisite, preserving the already authorized local-first/private-client gate distinction.

C implementation is this record's containing commit. Recovery profile 1 protects the person root plus supplied client state with fixed-cost Argon2id 64 MiB/three passes/one lane and XChaCha20Poly1305; canonical bounded input is checked before KDF work. Exports randomize salt/nonce; secret buffers/keys zeroize. One shared client engine admits one recovery operation. Import preserves person identity and receive-side state, then issues fresh signing devices/incarnations. It never returns an old active signer; later M13 integration still requires a fresh owner-authorized MLS join before sending. It recovers only actually exported state. Changing a kit password does not revoke other live devices.

Destination linking requires the current account session/CSRF proof plus a direct root signature bound to node/account/person/nonce. Client signing also checks expected destination context. Five-minute monotonic challenges have eight pending slots/account and expire on process restart. Link and accepted signed proof commit together; retries return the durable result after fresh session validation. Username collisions never merge people/accounts. Explicit unlink preserves signed history and prevents stale proof resurrection. Real source-loss testing removes the disposable source DB, decrypts a separately stored kit and links a newly authenticated destination; an unrelated colliding username retains its own person ID.

Node signing seed/public identity commit together in SQLite, eliminating a split key-file/database initialization state. Node identity survives restart; runtime incarnations are fresh and stale-context signing is rejected. Corrupt state fails without replacement. A real core restore contains the node key yet refuses signer startup under the persistent recovery marker. Unix live database/sidecars are private (0600); the core snapshot's private-file protection applies to its node seed. Client person roots remain client-held. M04 owns actual event scope/counter construction; M12 owns horizon reconciliation and restored-node activation.

Local Windows results: 68 workspace tests pass, including five recovery tests/one admission unit test, six link-service tests, three node-signer tests and a fixed link-proof byte vector, alongside all prior checks. Independent Node crypto/cbor-x verification reproduces both device and destination-link signatures/hashes and rejects tampering/wrong-account reuse. Format/clippy/roadmap validation pass. Linux adds two Unix-specific tests (70 expected total); actual C CI remains pending. Libraries include argon2 0.6.0 and chacha20poly1305 0.11.0 with zeroization enabled. Complete HTTP/browser/native-client, MLS state integration, restored-event counter checks and device acceptance remain downstream gates. Next: confirm C on Linux, then M04 A's signed event/authority/size vectors.

## 2026-10-31 — M03 provider complete; M04 A signed bytes and authority gates

The [C checkpoint CI](https://github.com/tellesus/BBSBOX/actions/runs/41172025063), commit 8f1767a, passed all three Ubuntu jobs (70 workspace tests including Unix permissions, independent device/link vectors, M00/browser/MLS/isolated-runtime regressions). **M03 is Complete as a provider.** Actual consumers own HTTP/native-client, scoped event counters, MLS and coordinated recovery integration; no release/device gate advances here.

Authority remains continued local development/U22. Baselines: roadmap 1.1.0/product 0.6.0, C01 v1.3.0/C02 v1.4.0, M03 brief 1.1.4/M04 brief 1.1.1. Sources: R11/R12/R13, P06/P21, AC07 and the existing M03 IDs. Rationale [ADR-0002](../decisions/ADR-0002-wire-profiles.md) is reference; choices are in the live contracts/briefs. Code/evidence is this record's containing commit.

Added the pure [event codec](../../crates/event-codec/README.md): deterministic CBOR/COSE, exact signed-byte IDs, actor/delegation checks, detached body hash/length, scoped stream hashes, typed event/credential/blob dependencies and bounded extensions. Identity/role/subject and audience/resource/distribution/incarnation are bound. Compiled kind/actor/purpose and required-extension gates precede current causal/resource policy. Signatures, actual bodies, permission to reduce and future committed integration/relevance/credit have distinct interfaces. Missing/unavailable lookups, unsupported versions/semantics and malformed bytes are distinct. Limits are 4 KiB envelope/64 KiB body, 32 dependencies, 16 extensions with 1,024-byte data under the overall budget. Root control contexts now also refresh on root creation/recovery and reject stale incarnations.

Standards review caught the deprecated polymorphic COSE EdDSA label (-8) in the early credential profile. New credential/link issuance uses versioned v2 domains/payloads and explicit Ed25519 (-19), following RFC 9864. Immutable v1 remains verification-only, with exact old IDs/bytes retained. Downgrading/relabeling fails; current challenges refuse old-profile proofs, while already accepted legacy receipts still retry. Public identities/cards, recovery kit format and SQL migrations remain unchanged. New event v1 starts with -19; the independent M00 v0 experiment remains historical evidence.

Local evidence: all prior workspace checks passed after profile changes; 15 codec tests and the additional legacy-receipt integration check pass, bringing Windows coverage to 86 tests (88 expected on Linux). Format/clippy/roadmap validation pass. Node/cbor-x independently reconstructs current v2 plus legacy v1 credential/link signatures, and event v1 metadata/envelope/stream hashes and signature, including u64::MAX/i64::MIN; tampering and wrong-resource/account reuse fail. The event vector's predecessor/dependency IDs are conformance fixtures, not accepted domain history. A's Linux confirmation remains pending until CI. M04 B/C and full AC07 remain open. Next: B, durable raw events/staging, duplicate results, fork evidence/isolation and real transactional reducer integration.
