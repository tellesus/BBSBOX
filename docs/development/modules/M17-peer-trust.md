# M17 — Local peer trust and exposure reporting

Version 1.1.0 · 2026-10-25 · Locked development baseline · Product baseline 0.6.0.

**State:** Approved for implementation under U19; unstarted. Runtime and hardware evidence remains pending.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C02](../contracts/C02-identity-events.md), [C03](../contracts/C03-exchange-delivery.md).
Look up only decision rows T13 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R04, R14, P08, P22, AC12. **Target gate:** G3. Scope summaries below preserve these source IDs; reconcile conflicts with the current product requirements and user instructions.

**Prerequisites:** [M14](M14-complaints-moderation.md), [M15](M15-selected-federation.md).

## Product outcome and boundaries

Let each SysOp independently assess signed evidence, contact a peer, restrict, banish or reinstate it. A client with cellular internet alone is not a violating node.

## Locked implementation

Use signed exposure-report records with subject node, reporter, observation type, evidence references and uncertainty. Local diagnostics inspect interfaces/routes/configuration only; they do not probe internet sites. Store local association policy separately from shared reports. Offer operator contact and explicit actions; no global vote threshold or automatic universal ban. Basic peer admission in M15 works before this richer report UI.

## Ownership and interfaces

Planned crates/trust/; evidence reports, operator contacts and local peer decisions. Raw private logs are not automatically exported as report evidence.

Paths above are planned implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: create/inspect a signed report with bounded local evidence.
2. B: forward/contact and independent local restrictions.
3. C: reinstatement, contradictory reports and delayed delivery.

## Completion evidence

- AC12 with two SysOps making different decisions about the same report.
- No infrastructure public connectivity probe occurs.
- A forged signer or client-only observation cannot masquerade as verified node exposure.

## Locked defaults and verification

Trust Ledger is a local case list, not a global reputation score. Observations are configuration/route evidence or signed human reports with confidence and references. Advisory/review-needed severity and a bounded priority control queue are selected; no automatic ban threshold. Operators explicitly restrict or reinstate peers. Public node cards show chosen name, key fingerprint, contact and advertised capabilities; private logs are not auto-shared.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
