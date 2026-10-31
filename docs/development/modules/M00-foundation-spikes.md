# M00 — Foundational feasibility and decisions

Version 1.1.0 · 2026-10-25 · Locked development baseline · Product baseline 0.6.0.

**State:** Approved under U19; In progress under the user’s 2026-10-30 build instruction. Local prototype evidence is recorded in WORK_LEDGER; G0-local/G2 remain open.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C02](../contracts/C02-identity-events.md), [C05](../contracts/C05-private-content.md).
Look up only decision rows T01, T02, T03, T04, T05, T06, T07, T08, T14 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R28, R03, R04, R05, R13, R15, R17, P04, P06, P07, P09, P21, AC01, AC03, AC11, AC14, AC15. **Target gate:** G0. Scope summaries below preserve these source IDs; reconcile conflicts with the current product requirements and user instructions.

**Prerequisites:** None; this is the first investigation unit.

## Product outcome and boundaries

Prove the few choices that could force broad redesign before building the full product. Preserve browser-first modern access and offline infrastructure; do not present a research build as a working private service.

## Locked implementation

Use disposable Rust/Axum/SQLite and static SvelteKit experiments. Test desktop HTTPS now; Android/iOS follow at G1/G2 with local trust enrollment. Compare independently signed clients, and test OpenMLS delayed epochs/competing commits with three clients. Upstream mobile/WASM builds are not device evidence. Measure reused hardware before promising a minimum.

## Ownership and interfaces

Current code is under spikes/m00/: a Rust/Axum/SQLite node and static SvelteKit page, plus TLS and browser checks. Its disposable schema/wire profile is not production protocol v1. See the [runbook](../../../spikes/m00/README.md). Publish supported device/access paths and measured idle/import/game resource use.

Only spikes/m00/ is implemented so far; later module locations remain planned. Consume the listed contracts and update affected consumers with any change.

## Sequential work slices

1. A: select available test hardware and devices; build a tiny offline page and signed-event round trip.
2. B: test fresh install, local address, TLS enrollment, key persistence and code-substitution boundary on phone and desktop.
3. C: test MLS disconnected writers, removal, old-epoch receipt, rejoin and conflicting commits; collect a restore proof.
4. D: record T01–T08/T14 decisions with measured results; update affected contracts before dependent production work.

## Completion evidence

- Every tested device loads all assets with WAN absent and produces no infrastructure internet attempts.
- Two disconnected clients can produce work that reconciles under the documented crypto policy without requiring every writer to stop.
- Strong host-confidentiality claims are backed by the actual client distribution demonstration, or remain blocked for that client class.

## Locked defaults and verification

Use the selected stack. Test Debian x86-64, desktop browsers, Android Chrome and iOS Safari; test the Tauri private-client path separately on each OS. Exact supported versions and capacity numbers come from results. G0-local needs desktop A/B and isolated-runtime evidence; failed private-client/MLS experiments are recorded blockers for G2, not blockers for independent local development. Pin dependency versions/lockfiles in the implementation commit, using maintained compatible releases.

## Current local evidence — 2026-10-30

The node/UI checks pass on Windows and Ubuntu CI. Eight OpenMLS fixtures additionally cover delayed/reordered work, removal, 32-epoch retention, commit forks and encrypted snapshots. Recovery is receive-only until a fresh owner-authorized join; raw MLS does not enforce forum-owner authority. Six Python checks validate the network auditor; the actual isolated Linux IPv4/IPv6 run is tracked in WORK_LEDGER. Phone tests wait for G1/G2 milestones. Trusted-client distribution and production admission/recovery remain open.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
