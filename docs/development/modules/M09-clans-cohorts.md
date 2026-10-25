# M09 — Local clans and cohorts

Version 0.1.0 · 2026-10-25 · Tier 2 proposal · Tier 1 baseline 0.4.0.

**State:** Planned capability; implementation choices proposed. No implementation or passed checks claimed.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C04](../contracts/C04-games.md), [C05](../contracts/C05-private-content.md).
Look up only decision rows T09, T10 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R21, R27, P17, P24, AC22. **Target gate:** G1. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M03](../modules/M03-identity-access.md), [M05](../modules/M05-interface-themes.md), [M07](../modules/M07-directory-profiles.md).

## Product outcome and boundaries

Users form local clans (larger organizations) and cohorts (smaller friend groups), each with a profile, directory and roster. No federation or automatic destination membership.

## Recommended implementation

Use a local group table with kind, home node, display data and membership records. Propose creator/manager/member roles, invitations with acceptance, voluntary leave and manager handover. Do not impose arbitrary numeric size limits or require a clan/cohort hierarchy. Choose roster visibility explicitly while retaining a discoverable local group page. Publish an authorized membership-query interface for games; it is available before passport transfers.

## Ownership and interfaces

Proposed crates/groups/; local groups, memberships, invitations and group profile configuration. Foreign identifiers may appear in historical game records but cannot create local memberships.

Paths above are proposed implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: create groups and expose local directory/profile/roster.
2. B: membership lifecycle and management handover.
3. C: integrate membership provider with M11/M18 and profile visibility.

## Completion evidence

- AC22: create a clan and cohort, manage accepted memberships and inspect their rosters.
- Copying a character or synchronizing a forum does not create/enroll a destination group.
- Membership access has a real integration test when the game provider is installed.

## Decisions before completion

Choose roster visibility and role details in T09; group chat, achievements and further fun features remain O12/P24 or O01/P03.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). A passing fixture is evidence for this module, not approval of new product scope. See [coverage](../COVERAGE.md) when changing source IDs.
