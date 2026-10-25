# M04 — Signed event kernel and convergence

Version 1.0.0 · 2026-10-25 · Locked development baseline · Tier 1 baseline 0.5.0.

**State:** Approved for implementation under U19; unstarted. Runtime and hardware evidence remains pending.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C01](../contracts/C01-storage-runtime.md), [C02](../contracts/C02-identity-events.md).
Look up only decision rows T04, T05 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R11, R12, R13, P06, P21, AC07. **Target gate:** G1. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M02](../modules/M02-storage-jobs.md), [M03](../modules/M03-identity-access.md).

## Product outcome and boundaries

Persist authored actions in a transport-neutral form before feature modules acquire incompatible local-only schemas. Signature validity and action authority remain separate.

## Locked implementation

Implement C02 with deterministic CBOR/COSE/Ed25519 candidate libraries, fixed signed-byte vectors and SHA-256 IDs. Use per-device hash-linked streams, explicit dependencies, immutable edits/tombstones and feature-specific reducers. No generic last-write-wins timestamp policy for governance. Initial gap detection is a range inventory, not Merkle machinery.

## Ownership and interfaces

Planned crates/event-core/ and protocol/fixtures/; raw events, stream heads, dependency staging, fork evidence and integration outcomes. Feature owners register kinds/reducers and relevance predicates.

Paths above are planned implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: encoder/decoder/signature/authority vectors and size limits.
2. B: durable staging, duplicate handling and fork isolation.
3. C: deterministic replay plus extension/version negotiation fixtures.

## Completion evidence

- Feed the same authorized history in many orders and obtain equal projections under equal policies.
- A newer edit followed by its older original never rolls back state.
- Unknown required kinds are unsupported rather than applied, and stream forks do not stop unrelated activity.

## Locked defaults and verification

Deterministic CBOR, COSE_Sign1, Ed25519 and SHA-256 are selected; JSON remains debug rendering only. Streams are scoped by device, audience/resource and writer incarnation (C02), preventing private-predecessor dependencies. Set explicit maximum event/chunk sizes in the schema and fixtures before public format v1; a canonical encoder library must pass independent vectors. Unknown critical fields/versions cannot be applied or credited.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved; genuinely new scope still follows change control. See [coverage](../COVERAGE.md) when changing source IDs.
