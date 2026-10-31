# O09 — Guest access and kiosk mode

Version 1.1.0 · 2026-10-25 · Locked development baseline · Product baseline 0.6.0.

**State:** Approved for implementation under U19; unstarted. Runtime and hardware evidence remains pending.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C01](../contracts/C01-storage-runtime.md), [C05](../contracts/C05-private-content.md).
Look up only decision rows T07, T09 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R28, P02, R03, R13, R26. **Target gate:** G5. **Expansion acceptance:** AC31. Scope summaries below preserve these source IDs; reconcile conflicts with the current product requirements and user instructions.

**Prerequisites:** [M05](M05-interface-themes.md), [M07](M07-directory-profiles.md), [M08](M08-friends-events.md), [M12](M12-maintenance-recovery.md).

## Product outcome and boundaries

Approved walk-up public browsing and session-reset interface. A kiosk is a client, not a new node authority.

## Locked implementation

Use a restricted browser route/layout with public queries, prominent local information and read-only public browsing. Authenticated personal use is outside this baseline. Reset sessions/client storage after sign-out and inactivity; private client keys must not persist in a public browser profile. Profile directory privacy applies identically.

## Ownership and interfaces

Planned web/kiosk/ and deploy/kiosk-client/; kiosk settings, local session reset and accessibility presentation.

Paths above are planned implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: choose public guest actions and build read-only route.
2. B: kiosk device navigation and session-reset exercise.
3. C: verify read-only baseline and session/storage reset; authenticated kiosk actions are outside this locked scope.

## Completion evidence

- Second visitor cannot retrieve the first visitor's private session or fields.
- Touch/keyboard navigation works on the chosen display.
- Client reset does not delete server accounts or change node privacy rules.

## Locked defaults and verification

Ship read-only public kiosk browsing first. Authenticated personal use is disabled in the baseline; the supported session-reset implementation clears browser state on exit/inactivity and restart. Use a normal supported browser/device in kiosk mode; no native kiosk app. This resolves the login option without weakening private-key protection.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
