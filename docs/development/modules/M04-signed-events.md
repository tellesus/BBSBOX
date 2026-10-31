# M04 — Signed event kernel and convergence

Brief revision 1.1.1 · Roadmap baseline 1.1.0 · 2026-10-31 · Product baseline 0.6.0.

**State:** In progress under U22 after M02/M03 provider completion. A has local codec/authority vectors; B/C remain pending. Consume C01 v1.3.0/C02 v1.4.0.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C01](../contracts/C01-storage-runtime.md), [C02](../contracts/C02-identity-events.md).
Look up only decision rows T04, T05 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R11, R12, R13, P06, P21, AC07. **Target gate:** G1. Scope summaries below preserve these source IDs; reconcile conflicts with the current product requirements and user instructions.

**Prerequisites:** [M02](M02-storage-jobs.md), [M03](M03-identity-access.md).

## Product outcome and boundaries

Persist authored actions in a transport-neutral form before feature modules acquire incompatible local-only schemas. Signature validity and action authority remain separate.

## Locked implementation

Implement C02 with deterministic CBOR/COSE/Ed25519 candidate libraries, fixed signed-byte vectors and SHA-256 IDs. Use per-device hash-linked streams, explicit dependencies, immutable edits/tombstones and feature-specific reducers. No generic last-write-wins timestamp policy for governance. Initial gap detection is a range inventory, not Merkle machinery.

## Ownership and interfaces

crates/event-codec/ implements A's pure signed-byte/authority boundaries; protocol/fixtures/ holds exact vectors. Planned crates/event-core/ owns B/C persistence, stream heads, staging, forks and integration outcomes. Features own reducers/relevance. See the [codec reference](../../../crates/event-codec/README.md).

Codec authorization permits reduction; it does not imply persisted integration or credit. Consume listed contracts and update affected consumers together. Disabled modules retain data; unsupported events stay uncredited.

## Sequential work slices

1. A: encoder/decoder/signature/authority vectors and size limits.
2. B: durable staging, duplicate handling and fork isolation.
3. C: deterministic replay plus extension/version negotiation fixtures.

## Completion evidence

- Feed the same authorized history in many orders and obtain equal projections under equal policies.
- A newer edit followed by its older original never rolls back state.
- Unknown required kinds are unsupported rather than applied, and stream forks do not stop unrelated activity.

## Locked defaults and verification

Event v1 uses deterministic CBOR/COSE_Sign1, explicit Ed25519 (-19), SHA-256 IDs and detached bodies; JSON is debug-only. Limits: 4 KiB envelope, 64 KiB body, 32 sorted typed dependencies, 16 unique extensions (1,024 bytes each, still within envelope budget). Actor roles are device/node/forum-root/person-root; kind rules enforce roles/device purposes before causal resource policy. Required extensions need registered validation; optional annotations grant no authority. The independent Node vector covers exact bytes and 64-bit boundaries. B owns durable staging/fork/commitment behavior; unsupported semantics remain uncredited.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
