# O08 — Dedicated relay installation

Version 1.0.0 · 2026-10-25 · Locked development baseline · Tier 1 baseline 0.5.0.

**State:** Approved for implementation under U19; unstarted. Runtime and hardware evidence remains pending.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C01](../contracts/C01-storage-runtime.md), [C03](../contracts/C03-exchange-delivery.md).
Look up only decision rows T01, T04 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R28, P01, P05, R04, R09, R13. **Target gate:** G5. **Expansion acceptance:** AC30. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M15](../modules/M15-selected-federation.md), [M17](../modules/M17-peer-trust.md).

## Product outcome and boundaries

Approved infrastructure profile that forwards permitted ciphertext/records without full community UI.

## Locked implementation

Reuse the node runtime and sync engine with only identity, policy, spool and local status enabled. Give relay queues explicit quotas and subscription routes. Forwarding does not grant group access. Start with known peers; opportunistic routing and multiple radios remain D07.

## Ownership and interfaces

Planned deploy/relay/; relay configuration and persistent spool. Uses existing protocol and crypto validators without adding a parallel format.

Paths above are planned implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: measure whether full-node-disabled mode already meets the need.
2. B: package the approved minimal preset; reuse the main binary.
3. C: forward across two offline links with restart/storage-limit fixtures.

## Completion evidence

- No internet attempts, full community accounts or plaintext private-group requirement.
- Forwarded records retain original signatures.
- A full spool yields an explicit retry/capacity result without false acceptance.

## Locked defaults and verification

Ship a relay deployment preset using the same binary with only identity, permitted exchange, spool and local status enabled. No full community UI or account directory. Configured peers/routes only; bounded queues and encrypted-at-rest spool. Advanced discovery/radio routing stays D07. A separate binary is unnecessary unless measurement justifies it.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved; genuinely new scope still follows change control. See [coverage](../COVERAGE.md) when changing source IDs.
