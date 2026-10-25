# M17 — Local peer trust and exposure reporting

Version 0.1.0 · 2026-10-25 · Tier 2 proposal · Tier 1 baseline 0.4.0.

**State:** Planned capability; implementation choices proposed. No implementation or passed checks claimed.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C02](../contracts/C02-identity-events.md), [C03](../contracts/C03-exchange-delivery.md).
Look up only decision rows T13 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R04, R14, P08, P22, AC12. **Target gate:** G3. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M14](../modules/M14-complaints-moderation.md), [M15](../modules/M15-selected-federation.md).

## Product outcome and boundaries

Let each SysOp independently assess signed evidence, contact a peer, restrict, banish or reinstate it. A client with cellular internet alone is not a violating node.

## Recommended implementation

Use signed exposure-report records with subject node, reporter, observation type, evidence references and uncertainty. Local diagnostics inspect interfaces/routes/configuration only; they do not probe internet sites. Store local association policy separately from shared reports. Offer operator contact and explicit actions; no global vote threshold or automatic universal ban. Basic peer admission in M15 works before this richer report UI.

## Ownership and interfaces

Proposed crates/trust/; evidence reports, operator contacts and local peer decisions. Raw private logs are not automatically exported as report evidence.

Paths above are proposed implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: create/inspect a signed report with bounded local evidence.
2. B: forward/contact and independent local restrictions.
3. C: reinstatement, contradictory reports and delayed delivery.

## Completion evidence

- AC12 with two SysOps making different decisions about the same report.
- No infrastructure public connectivity probe occurs.
- A forged signer or client-only observation cannot masquerade as verified node exposure.

## Decisions before completion

Choose supported observation types and manual default policy; severity/aggregation automation and public identity-card fields remain bounded P08/P22 design choices.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). A passing fixture is evidence for this module, not approval of new product scope. See [coverage](../COVERAGE.md) when changing source IDs.
