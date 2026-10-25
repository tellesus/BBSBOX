# M12 — Offline updates, backup and continuity

Version 0.1.0 · 2026-10-25 · Tier 2 proposal · Tier 1 baseline 0.4.0.

**State:** Planned capability; implementation choices proposed. No implementation or passed checks claimed.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C01](../contracts/C01-storage-runtime.md), [C02](../contracts/C02-identity-events.md).
Look up only decision rows T08, T12, T14 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R04, R13, R23, R24, P04, P07, P09, AC01, AC15, AC17. **Target gate:** G1. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M01](../modules/M01-offline-appliance.md), [M02](../modules/M02-storage-jobs.md), [M03](../modules/M03-identity-access.md), [M04](../modules/M04-signed-events.md).

## Product outcome and boundaries

Ship a real restore/update path with the first local alpha. Identity/key recovery and server restoration are separate tasks.

## Recommended implementation

Produce signed local release manifests, application/static assets and prepared dependency packages. The appliance never downloads an update. Snapshot database/blobs/configuration/node secrets consistently; keep client-key export separate. Validate signatures against an installed release trust root; distinguish release, node and person keys. Rotate restored writer incarnations before resuming when prior activity may exist. An update either migrates completely or restores a compatible snapshot without silently reusing counters.

## Ownership and interfaces

Proposed crates/maintenance/, deploy/releases/ and operator guides; release verification state, backup manifests, migration evidence and recovery/handover procedures.

Paths above are proposed implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: consistent backup and isolated restore with node identity/history checks.
2. B: offline verified install/update and interrupted migration recovery.
3. C: source-node-loss exercise, signing continuation policy and operator handover.

## Completion evidence

- AC15 restores accounts, content, files, pending jobs and credit receipts without duplicate effects.
- A server backup cannot be represented as recovery of absent client keys.
- AC17 records source/build materials and the still-explicit license decision.

## Decisions before completion

Define retention defaults, backup-secret protection, release-key rotation and recovery limitations. A previously lost root without recovery authorization cannot be reconstructed.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). A passing fixture is evidence for this module, not approval of new product scope. See [coverage](../COVERAGE.md) when changing source IDs.
