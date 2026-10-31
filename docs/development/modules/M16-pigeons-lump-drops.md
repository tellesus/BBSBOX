# M16 — Pigeon installation, lumps and credit

Version 1.1.0 · 2026-10-25 · Locked development baseline · Product baseline 0.6.0.

**State:** Approved for implementation under U19; unstarted. Runtime and hardware evidence remains pending.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C02](../contracts/C02-identity-events.md), [C03](../contracts/C03-exchange-delivery.md).
Look up only decision rows T04, T11 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R10, R11, R12, R25, P05, AC06, AC07, AC18. **Target gate:** G3. Scope summaries below preserve these source IDs; reconcile conflicts with the current product requirements and user instructions.

**Prerequisites:** [M07](M07-directory-profiles.md), [M15](M15-selected-federation.md).

## Product outcome and boundaries

Ship a lightweight courier installation for physical message carriage and the exact R25 count/privacy behavior. Rewards are excluded from this module.

## Locked implementation

Reuse Rust event/chunk codecs in a small courier executable with local queue/status UI. Use a laptop/Linux portable device as first active courier and a file-based lump importer/exporter as both a conformance fixture and the approved passive USB product workflow. C03 supplies signed sequence manifests, durable attribution and first-source provenance. Store event admission and unique receiving-delivery credit in one transactional flow. Count local receiver awards first; no network-wide totals claim.

## Ownership and interfaces

Planned crates/pigeon/, crates/delivery-credit/; courier queue/stamps, package manifests, receiving-delivery attribution, event provenance, signed receipts and private/public count projection.

Paths above are planned implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: dedicated courier collect/carry/deliver/return with no running full BBS.
2. B: signature/version/chunk checks and interrupted transfer.
3. C: authenticated delivering-user attribution, qualifying acceptance and retry-safe count.
4. D: integrate profile count privacy and pending-outcome UI.
5. E: passive USB workflow, configured multi-hop and Pigeon-to-Pigeon handoff, encrypted spool/media, route/status and capacity controls; no automatic opportunistic discovery.

## Completion evidence

- AC06/AC18 including old lump with one new event, all irrelevant, many new events, pending then accepted, restart/replay and two competing deliveries.
- Host accepts relevant encrypted envelope without carrier plaintext access.
- Private count is hidden from public profile/API but continues recording.

## Locked defaults and verification

Ship both a Linux active courier and passive signed-lump USB import/export workflow, then explicit configured multi-hop and Pigeon-to-Pigeon handoff. No automatic route discovery. Receiver-local count is labeled “Lump Drops at this BBS”; no aggregation. Visitors link/create a local account before credited import. Encrypted-at-rest courier spool/media is supported using OS encryption; payload signature checks remain mandatory. Pin provenance only after C03 admission checks; quarantine cannot reserve credit. Rewards belong to O12.

An opt-in status panel may show aggregate counts of explicitly public mail. Private-stream activity is excluded; this is a visibility control on the accepted queue/status feature, not a claim to expose encrypted contents.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
