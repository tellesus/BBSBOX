# O09 — Guest access and kiosk mode

Version 0.1.0 · 2026-10-25 · Tier 2 proposal · Tier 1 baseline 0.4.0.

**State:** Optional candidate; product approval required. No implementation or passed checks claimed.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C01](../contracts/C01-storage-runtime.md), [C05](../contracts/C05-private-content.md).
Look up only decision rows T07, T09 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** P02, R03, R13, R26. **Target gate:** G5. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M05](../modules/M05-interface-themes.md), [M07](../modules/M07-directory-profiles.md), [M08](../modules/M08-friends-events.md), [M12](../modules/M12-maintenance-recovery.md).

## Product outcome and boundaries

Conditional walk-up public browsing and session-reset interface. A kiosk is a client, not a new node authority.

## Recommended implementation

Use a restricted browser route/layout with public queries, prominent local information and explicit login/logout if personal use is allowed. Prefer read-only guest mode first. Reset sessions/client storage after sign-out and inactivity; private client keys must not persist in a public browser profile. Profile directory privacy applies identically.

## Ownership and interfaces

Proposed web/kiosk/ and deploy/kiosk-client/; kiosk settings, local session reset and accessibility presentation.

Paths above are proposed implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: choose public guest actions and build read-only route.
2. B: kiosk device navigation and session-reset exercise.
3. C: optional account actions only after key/session behavior is demonstrated.

## Completion evidence

- Second visitor cannot retrieve the first visitor's private session or fields.
- Touch/keyboard navigation works on the chosen display.
- Client reset does not delete server accounts or change node privacy rules.

## Decisions before completion

Choose supported kiosk hardware and whether login is allowed; guest access is not automatically anonymous posting.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). A passing fixture is evidence for this module, not approval of new product scope. See [coverage](../COVERAGE.md) when changing source IDs.
