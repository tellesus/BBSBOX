# M04 — Signed event kernel and convergence

Version 0.1.0 · 2026-10-25 · Tier 2 proposal · Tier 1 baseline 0.4.0.

**State:** Planned capability; implementation choices proposed. No implementation or passed checks claimed.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C01](../contracts/C01-storage-runtime.md), [C02](../contracts/C02-identity-events.md).
Look up only decision rows T04, T05 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R11, R12, R13, P06, P21, AC07. **Target gate:** G1. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M02](../modules/M02-storage-jobs.md), [M03](../modules/M03-identity-access.md).

## Product outcome and boundaries

Persist authored actions in a transport-neutral form before feature modules acquire incompatible local-only schemas. Signature validity and action authority remain separate.

## Recommended implementation

Implement C02 with deterministic CBOR/COSE/Ed25519 candidate libraries, fixed signed-byte vectors and SHA-256 IDs. Use per-device hash-linked streams, explicit dependencies, immutable edits/tombstones and feature-specific reducers. No generic last-write-wins timestamp policy for governance. Initial gap detection is a range inventory, not Merkle machinery.

## Ownership and interfaces

Proposed crates/event-core/ and protocol/fixtures/; raw events, stream heads, dependency staging, fork evidence and integration outcomes. Feature owners register kinds/reducers and relevance predicates.

Paths above are proposed implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: encoder/decoder/signature/authority vectors and size limits.
2. B: durable staging, duplicate handling and fork isolation.
3. C: deterministic replay plus extension/version negotiation fixtures.

## Completion evidence

- Feed the same authorized history in many orders and obtain equal projections under equal policies.
- A newer edit followed by its older original never rolls back state.
- Unknown required kinds are unsupported rather than applied, and stream forks do not stop unrelated activity.

## Decisions before completion

Finalize exact signed fields, schema IDs and algorithm identifiers; measure encoding versus canonical JSON as the fallback before freezing protocol v1.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). A passing fixture is evidence for this module, not approval of new product scope. See [coverage](../COVERAGE.md) when changing source IDs.
