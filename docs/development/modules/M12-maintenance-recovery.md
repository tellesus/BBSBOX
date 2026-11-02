# M12 — Offline updates, backup and continuity

Brief revision 1.1.5 · Roadmap baseline 1.1.0 · 2026-11-01 · Locked scope · Product baseline 0.6.0.

**State:** In progress under continued development authority. C01 v1.4.0 maintenance/barrier and encrypted archives pass local/Linux checks. Current foundation-node capture/operator backup and isolated restore pass local checks; actual guest integration awaits CI. Game checkpoints, releases and recovered activation remain open.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C01](../contracts/C01-storage-runtime.md), [C02](../contracts/C02-identity-events.md).
Look up only decision rows T08, T12, T14 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R04, R13, R23, R24, P04, P07, P09, AC01, AC15, AC17. **Target gate:** G1. Scope summaries below preserve these source IDs; reconcile conflicts with the current product requirements and user instructions.

**Prerequisites:** [M01](M01-offline-appliance.md), [M02](M02-storage-jobs.md), [M03](M03-identity-access.md), [M04](M04-signed-events.md).

## Product outcome and boundaries

Ship a real restore/update path with the first local alpha. Identity/key recovery and server restoration are separate tasks.

## Locked implementation

Produce signed local release manifests, application/static assets and prepared dependency packages. The appliance never downloads an update. Snapshot database/blobs/configuration/node secrets consistently; keep client-key export separate. Validate signatures against an installed release trust root; distinguish release, node and person keys. Rotate restored writer incarnations before resuming when prior activity may exist. An update either migrates completely or restores a compatible snapshot without silently reusing counters.

## Ownership and interfaces

Current crates/maintenance/ archive primitives; planned deploy/releases/ and operator guides; release verification state, backup manifests, migration evidence and recovery/handover procedures.

The [maintenance reference](../../../crates/maintenance/README.md) records current implementation details. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: consistent backup and isolated restore with node identity/history checks. First add a writer maintenance barrier that drains accepted mutations before checkpoints; then coordinate core/configuration/checkpoint inventory and protected exports.
2. B: offline verified install/update and interrupted migration recovery.
3. C: source-node-loss exercise, signing continuation policy and operator handover.

## Completion evidence

- AC15 restores all installed core/game data and jobs consistently. Use receipt fixtures now; M16 owns real credit-ledger restore/replay checks later. Lost post-snapshot writes follow C01 recovery limits.
- A server backup cannot be represented as recovery of absent client keys.
- AC17 records source/build materials, the selected T12 licenses and matching notices/corresponding source.

## Locked defaults and verification

Use signed offline release bundles and snapshot manifests, encrypted removable backup media, and explicit operator restore/handover instructions. Default schedule: daily snapshot when configured backup media is available; keep seven daily and four weekly snapshots subject to capacity warnings. Do not erase the only good snapshot. Separate consistent checkpoints include game-worker databases. Lost post-snapshot writes remain explicitly unrecovered until reconciled; no universal zero-loss/exactly-once-after-data-loss promise.

Implementation selection under continued development authority: age X25519 recipient encryption with a separate operator backup key; a node-signed, domain-separated inventory authenticates exact staged paths/sizes/hashes. Verify expected node identity and the complete encrypted stream before completing extraction. Full-node coordination and recovery blocking remain mandatory beyond archive verification.

First operator path: stop/drain the service for a cold maintenance window, serialize configuration against TLS renewal, require the exact existing node/schema, and stage database/blobs/configuration/server secrets privately as root. Repair only known source database/sidecar/lock ownership before resuming a previously active service. Restore into fresh isolated storage with persistent recovery blocking and staged configuration. The current profile admits identity/events owners with no games; new owners/checkpoints require explicit backup integration. Scheduling/retention and recovery continuation remain separate work.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
