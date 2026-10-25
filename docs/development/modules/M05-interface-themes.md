# M05 — Browser shell, navigation and SysOp themes

Version 0.1.0 · 2026-10-25 · Tier 2 proposal · Tier 1 baseline 0.4.0.

**State:** Planned capability; implementation choices proposed. No implementation or passed checks claimed.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C01](../contracts/C01-storage-runtime.md), [C05](../contracts/C05-private-content.md).
Look up only decision rows T03, T07 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R01, R02, R03, R06, P03, AC03. **Target gate:** G1. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M01](../modules/M01-offline-appliance.md), [M03](../modules/M03-identity-access.md).

## Product outcome and boundaries

Provide modern phone/tablet/desktop navigation with a shipped vibrant-on-black dark theme, regardless of device light preference. SysOps customize the local place.

## Recommended implementation

Prefer SvelteKit/TypeScript with static adapter output served by the node; all assets/fonts are bundled. Responsive routes for home, people, discussions, events, mail and games expose only installed capabilities. Use design tokens and a declarative menu/theme manifest. Keep semantic HTML, keyboard focus and readable contrast under customization; no arbitrary theme scripts in the private client.

## Ownership and interfaces

Proposed web/shell/, web/theme/ and crates/settings/; local theme/navigation config, welcome art and node identity presentation. Consumes feature navigation/profile contributions without owning their data.

Paths above are proposed implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: local landing/login navigation and responsive page skeleton.
2. B: shipped palette, theme tokens and SysOp preview/apply.
3. C: clear empty, pending, disconnected and unavailable-module states.

## Completion evidence

- AC03 on touch, mouse and keyboard; OS light setting does not change shipped default.
- A new visitor identifies the place/SysOp and finds an event without understanding federation.
- Network capture finds no remote font/script/image dependency.

## Decisions before completion

Choose initial palette, menu customization limits and the exact independently trusted client presentation surface from M00; welcome/MOTD refinements remain P03.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). A passing fixture is evidence for this module, not approval of new product scope. See [coverage](../COVERAGE.md) when changing source IDs.
