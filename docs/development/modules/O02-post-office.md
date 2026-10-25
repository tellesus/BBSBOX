# O02 — Encrypted cross-node Post Office

Version 0.1.0 · 2026-10-25 · Tier 2 proposal · Tier 1 baseline 0.4.0.

**State:** Optional candidate; product approval required. No implementation or passed checks claimed.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C02](../contracts/C02-identity-events.md), [C05](../contracts/C05-private-content.md).
Look up only decision rows T06, T07 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** P11, R10, R12, R13, R15. **Target gate:** G5. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M13](../modules/M13-encrypted-forums.md), [M15](../modules/M15-selected-federation.md), [M16](../modules/M16-pigeons-lump-drops.md).

## Product outcome and boundaries

Conditional extension of local mail to encrypted cross-node correspondence with honest delivery state.

## Recommended implementation

Prefer reusing the proven M13 group engine for two-party or explicitly addressed small conversations, rather than inventing another encryption protocol. Define recipient key discovery and prepublished invitation/key material over signed records. Reuse C03 receipt states. Address identity separately from current hosting node; offline recipients remain pending until a route exists.

## Ownership and interfaces

Proposed crates/post-office/; encrypted mailbox routing, conversation references and delivery projections. Keys remain on clients.

Paths above are proposed implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: decide addressing, invitation and historical-key retention.
2. B: two-node message/reply via Pigeon.
3. C: recipient move, missing key material and delayed acknowledgments.

## Completion evidence

- Courier and host cannot read contents under the selected client model.
- A receipt cannot be displayed as read acknowledgment.
- A destination change cannot redirect plaintext to a similarly named local account.

## Decisions before completion

Approve cross-node mail itself plus first recipient-discovery and conversation-retention policy.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). A passing fixture is evidence for this module, not approval of new product scope. See [coverage](../COVERAGE.md) when changing source IDs.
