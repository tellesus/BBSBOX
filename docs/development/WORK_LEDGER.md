# Development work ledger

Version 1.0.1 · 2026-10-25 · Locked roadmap; M00 In progress, remaining modules Unstarted. Execution updated 2026-10-30.

Evidence log — not an independent source of scope authority. Current choices live in the canonical roadmap and user instructions.

Design review is complete and the first M00 prototype has real local implementation evidence recorded below. Approval does not move a module to Complete; only its required checks do.

| Module | Scope | Execution state | Evidence / commit | Next action |
| --- | --- | --- | --- | --- |
| [M00](modules/M00-foundation-spikes.md) | Approved | In progress | 2026-10-30: Rust/API/store and HTTPS browser checks pass; see run record below | Physical-device B, MLS/client C, measurements and D; G0-local/G2 open |
| [M01](modules/M01-offline-appliance.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
| [M02](modules/M02-storage-jobs.md) | Approved | Unstarted | No runtime evidence | Begin A after applicable prerequisite evidence |
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
