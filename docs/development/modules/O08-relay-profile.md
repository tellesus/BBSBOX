# O08 — Dedicated relay installation

Version 0.1.0 · 2026-10-25 · Tier 2 proposal · Tier 1 baseline 0.4.0.

**State:** Optional candidate; product approval required. No implementation or passed checks claimed.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C01](../contracts/C01-storage-runtime.md), [C03](../contracts/C03-exchange-delivery.md).
Look up only decision rows T01, T04 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** P01, P05, R04, R09, R13. **Target gate:** G5. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M15](../modules/M15-selected-federation.md), [M17](../modules/M17-peer-trust.md).

## Product outcome and boundaries

Conditional infrastructure profile that forwards permitted ciphertext/records without full community UI.

## Recommended implementation

Reuse the node runtime and sync engine with only identity, policy, spool and local status enabled. Give relay queues explicit quotas and subscription routes. Forwarding does not grant group access. Start with known peers; opportunistic routing and multiple radios remain D07.

## Ownership and interfaces

Proposed deploy/relay/; relay configuration and persistent spool. Uses existing protocol and crypto validators without adding a parallel format.

Paths above are proposed implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: measure whether full-node-disabled mode already meets the need.
2. B: package minimal profile if justified.
3. C: forward across two offline links with restart/storage-limit fixtures.

## Completion evidence

- No internet attempts, full community accounts or plaintext private-group requirement.
- Forwarded records retain original signatures.
- A full spool yields an explicit retry/capacity result without false acceptance.

## Decisions before completion

Approve a separate relay profile only after a measured benefit and an actual deployment need.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). A passing fixture is evidence for this module, not approval of new product scope. See [coverage](../COVERAGE.md) when changing source IDs.
