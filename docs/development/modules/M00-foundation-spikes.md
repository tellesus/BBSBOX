# M00 — Foundational feasibility and decisions

Version 1.0.1 · 2026-10-25 · Locked development baseline · Product baseline 0.5.1.

**State:** Approved for implementation under U19; unstarted. Runtime and hardware evidence remains pending.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C02](../contracts/C02-identity-events.md), [C05](../contracts/C05-private-content.md).
Look up only decision rows T01, T02, T03, T04, T05, T06, T07, T08, T14 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R28, R03, R04, R05, R13, R15, R17, P04, P06, P07, P09, P21, AC01, AC03, AC11, AC14, AC15. **Target gate:** G0. Scope summaries below preserve these source IDs; reconcile conflicts with the current product requirements and user instructions.

**Prerequisites:** None; this is the first investigation unit.

## Product outcome and boundaries

Prove the few choices that could force broad redesign before building the full product. Preserve browser-first modern access and offline infrastructure; do not present a research build as a working private service.

## Locked implementation

Use disposable Rust/Axum + SQLite and static SvelteKit experiments. Exercise HTTPS on actual Android/iOS and desktop browsers with locally supplied trust material; compare independently signed bundled clients for private keys. Prototype OpenMLS delayed epochs and competing governance commits with three clients. Its Android/iOS/WASM targets are built but not tested by upstream CI; our device evidence must establish support. Benchmark a candidate reused machine rather than promise a hardware minimum.

## Ownership and interfaces

Experiments under planned spikes/; fixture traces and decision evidence, no production schema. Publish supported device/access paths and measured idle/import/game resource use.

Paths above are planned implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

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

Use the selected stack. Test Debian x86-64, desktop browsers, Android Chrome and iOS Safari; test the Tauri private-client path separately on each OS. Exact supported versions and capacity numbers come from results. G0-local needs A/B access and storage/signature evidence; failed private-client/MLS experiments are recorded blockers for G2, not blockers for independent local development. Pin dependency versions/lockfiles in the implementation commit, using maintained compatible releases.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
