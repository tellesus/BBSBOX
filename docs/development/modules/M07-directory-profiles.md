# M07 — Member directory and chosen profile displays

Version 1.0.0 · 2026-10-25 · Locked development baseline · Tier 1 baseline 0.5.0.

**State:** Approved for implementation under U19; unstarted. Runtime and hardware evidence remains pending.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C01](../contracts/C01-storage-runtime.md), [C05](../contracts/C05-private-content.md).
Look up only decision rows T09, T11, T12 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R25, R26, P03, P24, AC18, AC20, AC21. **Target gate:** G1. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M03](../modules/M03-identity-access.md), [M05](../modules/M05-interface-themes.md).

## Product outcome and boundaries

List every local account, including private profiles, with its common public baseline. Optional contact/bio/message, games/characters, hosted events and feed links honor user choices. Lump Drop visibility is independent.

## Locked implementation

Use username plus stable profile link as the minimal common baseline. Typed field definitions distinguish optional contact details from baseline fields. Use server-side field filtering and a profile-component provider interface for games, events and drop counts. Providers may be absent until their module ships; do not display fabricated zeroes or capabilities. Profile privacy hides optional details; count privacy follows R25.

## Ownership and interfaces

Planned crates/profiles/; local directory projection, field visibility, profile-component settings. Owns presentation preferences, not game state, events or delivery credits.

Paths above are planned implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: account directory, private/public profiles and editable optional fields.
2. B: profile component contract with fixtures for absent/hidden data.
3. C: provide and test provider registration/visibility hooks with fixtures. M08, M11/M18 and M16 own their later live bindings and repeat AC20/AC21/AC18 when they ship; those future integrations do not block completion of this directory foundation.

## Completion evidence

- AC20 verifies hidden data is absent from responses, not merely the rendered screen.
- Every private/public local account is findable by username.
- AC21/AC18 provider checks rerun when the corresponding module is delivered.

## Locked defaults and verification

Directory baseline is username plus stable local profile link. Optional public fields: bio, short message, email, phone and contact/address text, each explicitly published by the user. Optional fields default hidden; no automatic contact lookup. Profiles default private; the separate Lump Drop counter remains public by default under R25. Feed/event/game providers enforce their own audience on the server, and later consumer modules own live bindings.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved; genuinely new scope still follows change control. See [coverage](../COVERAGE.md) when changing source IDs.
