# M07 — Member directory and chosen profile displays

Brief revision 1.1.1 · 2026-11-03 · Roadmap baseline 1.1.0 · Product baseline 0.6.0.

**State:** In progress. A implements the local account directory and profile editor. B–C add component hooks and tests.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C01](../contracts/C01-storage-runtime.md), [C05](../contracts/C05-private-content.md).
Look up only decision rows T09, T11, T12 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R25, R26, P03, P24, AC18, AC20, AC21. **Target gate:** G1. Scope summaries below preserve these source IDs; reconcile conflicts with the current product requirements and user instructions.

**Prerequisites:** [M03](M03-identity-access.md), [M05](M05-interface-themes.md).

## Product outcome and boundaries

List every local account, including private profiles, with its common public baseline. Optional contact/bio/message, games/characters, hosted events and feed links honor user choices. Lump Drop visibility is independent.

## Locked implementation

Use username plus stable profile link as the minimal common baseline. Typed field definitions distinguish optional contact details from baseline fields. Use server-side field filtering and a profile-component provider interface for games, events and drop counts. Providers may be absent until their module ships; do not display fabricated zeroes or capabilities. Profile privacy hides optional details; count privacy follows R25.

## Ownership and interfaces

`crates/profiles/` owns local profile field visibility and profile-component settings. The directory reads usernames from M03. Game state, events and delivery credits stay with their providers.

Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and leaves unrelated services available.

## Sequential work slices

1. A: account directory, private/public profiles and editable optional fields.
2. B: profile component contract with fixtures for absent/hidden data.
3. C: provide and test provider registration/visibility hooks with fixtures. M08, M11/M18 and M16 own their later live bindings and repeat AC20/AC21/AC18 when they ship; those future integrations do not block completion of this directory foundation.

## Completion evidence

- AC20 verifies hidden data is absent from responses, not merely the rendered screen.
- Every private/public local account is findable by username.
- AC21/AC18 provider checks rerun when the corresponding module is delivered.

## Locked defaults and verification

Signed-in members can browse or find every local username and follow its stable profile link. Owners edit bio, short message, email, phone and contact/address text and can choose which fields to publish. Profiles and fields default private. The separate Lump Drop counter remains public by default under R25. Feed/event/game providers enforce their own audience on the server, and later consumer modules own live bindings.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
