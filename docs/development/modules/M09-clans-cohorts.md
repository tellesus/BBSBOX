# M09 — Local clans and cohorts

Version 1.1.0 · 2026-10-25 · Locked development baseline · Product baseline 0.6.0.

**State:** Approved for implementation under U19; unstarted. Runtime and hardware evidence remains pending.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C04](../contracts/C04-games.md), [C05](../contracts/C05-private-content.md).
Look up only decision rows T09, T10 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R21, R27, P17, P24, AC22. **Target gate:** G1. Scope summaries below preserve these source IDs; reconcile conflicts with the current product requirements and user instructions.

**Prerequisites:** [M03](M03-identity-access.md), [M05](M05-interface-themes.md), [M07](M07-directory-profiles.md).

## Product outcome and boundaries

Users form local clans (larger organizations) and cohorts (smaller friend groups), each with a profile, directory and roster. No federation or automatic destination membership.

## Locked implementation

Use a local group table with kind, home node, display data and membership records. Use creator/manager/member roles, invitations with acceptance, voluntary leave and manager handover. Do not impose arbitrary numeric size limits or require a clan/cohort hierarchy. Choose roster visibility explicitly while retaining a discoverable local group page. Publish an authorized membership-query interface for games; it is available before passport transfers.

## Ownership and interfaces

Planned crates/groups/; local groups, memberships, invitations and group profile configuration. Foreign identifiers may appear in historical game records but cannot create local memberships.

Paths above are planned implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: create groups and expose local directory/profile/roster.
2. B: membership lifecycle and management handover.
3. C: finish membership/profile API fixtures. M11/M18 own real game integration later; their implementation is not a reverse prerequisite for M09.

## Completion evidence

- AC22: create a clan and cohort, manage accepted memberships and inspect their rosters.
- Copying a character or synchronizing a forum does not create/enroll a destination group.
- Membership access has a real integration test when the game provider is installed.

## Locked defaults and verification

Creator/manager/member roles, accepted invitations, voluntary leave and explicit manager handover are selected; no fixed clan/cohort sizes or hierarchy. Directory/profile are public to local members; roster visibility defaults to members and can be made public by managers with clear joining notice. M09 supplies local membership fixtures/API; M11/M18 own later game binding. A last manager must hand over or dissolve the local group.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
