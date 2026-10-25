# M16 — Pigeon installation, lumps and credit

Version 0.1.0 · 2026-10-25 · Tier 2 proposal · Tier 1 baseline 0.4.0.

**State:** Planned capability; implementation choices proposed. No implementation or passed checks claimed.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C02](../contracts/C02-identity-events.md), [C03](../contracts/C03-exchange-delivery.md).
Look up only decision rows T04, T11 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R10, R11, R12, R25, P05, AC06, AC07, AC18. **Target gate:** G3. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M07](../modules/M07-directory-profiles.md), [M15](../modules/M15-selected-federation.md).

## Product outcome and boundaries

Ship a lightweight courier installation for physical message carriage and the exact R25 count/privacy behavior. Rewards are excluded from this module.

## Recommended implementation

Reuse Rust event/chunk codecs in a small courier executable with local queue/status UI. Propose a laptop/Linux portable device as first active courier and a file-based lump importer/exporter as a conformance fixture; passive USB as a product workflow requires the P05 decision. C03 supplies signed sequence manifests, durable attribution and first-source provenance. Store event admission and unique receiving-delivery credit in one transactional flow. Count local receiver awards first; no network-wide totals claim.

## Ownership and interfaces

Proposed crates/pigeon/, crates/delivery-credit/; courier queue/stamps, package manifests, receiving-delivery attribution, event provenance, signed receipts and private/public count projection.

Paths above are proposed implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: dedicated courier collect/carry/deliver/return with no running full BBS.
2. B: signature/version/chunk checks and interrupted transfer.
3. C: authenticated delivering-user attribution, qualifying acceptance and retry-safe count.
4. D: integrate profile count privacy and pending-outcome UI.

## Completion evidence

- AC06/AC18 including old lump with one new event, all irrelevant, many new events, pending then accepted, restart/replay and two competing deliveries.
- Host accepts relevant encrypted envelope without carrier plaintext access.
- Private count is hidden from public profile/API but continues recording.

## Decisions before completion

Confirm first hardware/workflow, attribution of nonresident carriers, retention limits and local-count label. Aggregated totals and discretionary rewards need their own later decisions.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). A passing fixture is evidence for this module, not approval of new product scope. See [coverage](../COVERAGE.md) when changing source IDs.
