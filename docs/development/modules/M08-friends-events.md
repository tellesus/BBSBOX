# M08 — Chosen feeds, gatherings and invitations

Version 0.1.0 · 2026-10-25 · Tier 2 proposal · Tier 1 baseline 0.4.0.

**State:** Planned capability; implementation choices proposed. No implementation or passed checks claimed.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C02](../contracts/C02-identity-events.md), [C05](../contracts/C05-private-content.md).
Look up only decision rows T09 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R18, R19, R26, P10, AC08, AC09, AC20. **Target gate:** G1. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M04](../modules/M04-signed-events.md), [M05](../modules/M05-interface-themes.md), [M07](../modules/M07-directory-profiles.md).

## Product outcome and boundaries

Provide chronological chosen posts and event announcements/invitations without ranking or paid reach. Event dates are meaningful human dates but never protocol authority.

## Recommended implementation

Propose explicit following for public posts, with mutual confirmed friendship as a distinct optional audience relationship to decide before private sharing. First slice supports public local updates, selected recipients for invitations, event title/place/date/timezone/description and authored revisions. Sort by claimed authored time plus stable ID, flag late arrival/clock uncertainty, and never let clock order resolve edits. A calendar/RSVP slice proposes going/maybe/not-going and last-known counts; reminders are local due work only, no cloud push.

## Ownership and interfaces

Proposed crates/social/ and crates/gatherings/; relationship choices, feed projection, event revisions, invitations and optional RSVP state. M13 is required for any promise of host-blind audience-restricted sharing.

Paths above are proposed implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: chosen public feed and follow/unfollow, with chronological fixtures.
2. B: create/update/cancel an event and address invitations; expose profile links.
3. C: if P10 details are approved, add calendar/RSVP/local reminders and stale-state labels.

## Completion evidence

- AC08/AC09: late invitations reach the appropriate view without implying they were read.
- An arriving original event cannot undo a known cancellation.
- A resident finds a game night through a profile or event list.

## Decisions before completion

Record friend/follow and private-audience semantics, display-time correction policy and RSVP scope in T09; private sharing cannot quietly become plaintext server storage.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). A passing fixture is evidence for this module, not approval of new product scope. See [coverage](../COVERAGE.md) when changing source IDs.
