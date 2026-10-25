# M08 — Chosen feeds, gatherings and invitations

Version 1.0.1 · 2026-10-25 · Locked development baseline · Product baseline 0.5.1.

**State:** Approved for implementation under U19; unstarted. Runtime and hardware evidence remains pending.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C02](../contracts/C02-identity-events.md), [C05](../contracts/C05-private-content.md).
Look up only decision rows T09 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R18, R19, R26, P10, AC08, AC09, AC20. **Target gate:** G1. Scope summaries below preserve these source IDs; reconcile conflicts with the current product requirements and user instructions.

**Prerequisites:** [M04](M04-signed-events.md), [M05](M05-interface-themes.md), [M07](M07-directory-profiles.md).

## Product outcome and boundaries

Provide chronological chosen posts and event announcements/invitations without ranking or paid reach. Event dates are meaningful human dates but never protocol authority.

## Locked implementation

Use explicit following for public posts, with mutual confirmed friendship as a distinct selected audience relationship; M13 implements its encrypted sharing adapter. First slice supports public local updates, selected recipients for invitations, event title/place/date/timezone/description and authored revisions. Sort by claimed authored time plus stable ID, flag late arrival/clock uncertainty, and never let clock order resolve edits. The calendar/RSVP slice implements going/maybe/not-going and last-known counts; reminders are local due work only, no cloud push.

## Ownership and interfaces

Planned crates/social/ and crates/gatherings/; relationship choices, feed projection, event revisions, invitations and optional RSVP state. M13 is required for any promise of host-blind audience-restricted sharing.

Paths above are planned implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: chosen public feed and follow/unfollow, with chronological fixtures.
2. B: create/update/cancel an event and address invitations; expose profile links.
3. C: add calendar/RSVP/local reminders and stale-state labels.

## Completion evidence

- AC08/AC09: late invitations reach the appropriate view without implying they were read.
- An arriving original event cannot undo a known cancellation.
- A resident finds a game night through a profile or event list.

## Locked defaults and verification

Directional public following is selected. Use descending claimed authored-time with event-ID tie-break and explicit late-arrival/clock-uncertainty labels; edits use causal revision rules. Calendar, going/maybe/not-going RSVPs, cancellation and local reminders are approved. Mutual confirmed friendships are a relationship separate from follows. G1 implements public feeds/events and recipient-addressed host-visible invitations; M13 owns the G2 encrypted friend-audience adapter, so no private-content promise leaks into G1.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
