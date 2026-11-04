# M08 — Chosen feeds, gatherings and invitations

Brief revision 1.1.5 · 2026-11-03 · Roadmap baseline 1.1.0 · Product baseline 0.6.0.

**State:** In progress. A–C pass locally. Signed Debian confirmation is pending for the full module.

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

`crates/social/` owns follows, friends and feeds. `crates/gatherings/` owns events, invitations, RSVPs and reminders. M13 owns encrypted friend audiences.

Follow the listed contracts. A disabled optional module retains its data and leaves other services available.

## Sequential work slices

1. A: chosen public feed and follow/unfollow, with chronological fixtures.
2. B: confirm friendships, create/update/cancel gatherings and address invitations. Expose profile links.
3. C: add calendar/RSVP/local reminders and stale-state labels.

## Completion evidence

- AC08/AC09: late invitations reach the appropriate view without implying they were read.
- An arriving original event cannot undo a known cancellation.
- A resident finds a game night through a profile or event list.

## Locked defaults and verification

Following selects public updates. Sort by descending authored time, then event ID. Label five-minute arrival or clock differences. Gatherings store local time, IANA zone and the resolved instant. Edits follow causal revisions. Mutual friendship is separate. Calendar, RSVP, cancellation and local reminders are approved. G1 invitations are host-visible and recipient-addressed.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
