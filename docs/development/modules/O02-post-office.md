# O02 — Encrypted cross-node Post Office

Version 1.0.1 · 2026-10-25 · Locked development baseline · Product baseline 0.5.1.

**State:** Approved for implementation under U19; unstarted. Runtime and hardware evidence remains pending.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C03](../contracts/C03-exchange-delivery.md), [C05](../contracts/C05-private-content.md).
Look up only decision rows T06, T07 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R28, P11, R10, R12, R13, R15. **Target gate:** G5. **Expansion acceptance:** AC24. Scope summaries below preserve these source IDs; reconcile conflicts with the current product requirements and user instructions.

**Prerequisites:** [M13](M13-encrypted-forums.md), [M15](M15-selected-federation.md), [M16](M16-pigeons-lump-drops.md).

## Product outcome and boundaries

Approved extension of local mail to encrypted cross-node correspondence with honest delivery state.

## Locked implementation

Use reusing the proven M13 group engine for two-party or explicitly addressed small conversations, rather than inventing another encryption protocol. Define recipient key discovery and prepublished invitation/key material over signed records. Reuse C03 receipt states. Address identity separately from current hosting node; offline recipients remain pending until a route exists.

## Ownership and interfaces

Planned crates/post-office/; encrypted mailbox routing, conversation references and delivery projections. Keys remain on clients.

Paths above are planned implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: decide addressing, invitation and historical-key retention.
2. B: two-node message/reply via Pigeon.
3. C: recipient move, missing key material and delayed acknowledgments.

## Completion evidence

- Courier and host cannot read contents under the selected client model.
- A receipt cannot be displayed as read acknowledgment.
- A destination change cannot redirect plaintext to a similarly named local account.

## Locked defaults and verification

Use M13’s selected engine for addressed encrypted conversations, default two-party with explicitly invited small groups. Public signed prekey/contact records and identity-bound routing support discovery; missing keys remain pending. A move requires signed identity proof, not username matching. Reuse C03 delivery states and client-side history retention/export; no automatic read receipts. Cryptographic state and retention remain subject to the same G2 checks.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
