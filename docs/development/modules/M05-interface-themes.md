# M05 — Browser shell, navigation and SysOp themes

Brief revision 1.1.3 · 2026-11-03 · Roadmap baseline 1.1.0 · Product baseline 0.6.0.

**State:** In progress. Slice A landing, sign-in and account work over local HTTPS. Debian release rebuild and G1 device evidence remain pending.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C01](../contracts/C01-storage-runtime.md), [C05](../contracts/C05-private-content.md).
Look up only decision rows T03, T07 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R01, R02, R03, R06, P03, AC03. **Target gate:** G1. Scope summaries below preserve these source IDs; reconcile conflicts with the current product requirements and user instructions.

**Prerequisites:** [M01](M01-offline-appliance.md), [M03](M03-identity-access.md).

## Product outcome and boundaries

Provide modern phone/tablet/desktop navigation with a shipped vibrant-on-black dark theme, regardless of device light preference. SysOps customize the local place.

Use short labels, actions and status descriptions. Omit motivational headlines, slogans and aspirational filler. The visual design gets a separate review once the working flow is available.

## Locked implementation

Use SvelteKit/TypeScript with static adapter output served by the node; all assets/fonts are bundled. Responsive routes for home, people, discussions, events, mail and games expose only installed capabilities. Use design tokens and a declarative menu/theme manifest. Keep semantic HTML, keyboard focus and readable contrast under customization; no arbitrary theme scripts in the private client.

## Ownership and interfaces

The browser shell lives in `web/shell/`. Planned `web/theme/` and `crates/settings/` own local theme/navigation config and welcome art. The shell consumes feature navigation/profile contributions without owning their data.

The [runtime](../../../crates/node-runtime/README.md#local-account-access) owns HTTP sessions and private first-SysOp setup. The shell consumes that API. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: local landing/login navigation and responsive page skeleton.
2. B: shipped palette, theme tokens and SysOp preview/apply.
3. C: clear empty, pending, disconnected and unavailable-module states.

## Completion evidence

- AC03 on touch, mouse and keyboard; OS light setting does not change shipped default.
- A new visitor identifies the place/SysOp and finds an event without understanding federation.
- Network capture finds no remote font/script/image dependency.

## Locked defaults and verification

Use SvelteKit static output and TypeScript, system fonts, black #000000 background, near-white #F5F5F5 text, cyan #00E5FF and magenta #FF4FD8 accents as the initial editable palette. Contrast/focus are verified before release. Themes contain data/CSS tokens and sanitized art, not executable private-client scripts. Core owns welcome/MOTD rendering; O01 adds scheduling/presence utilities without duplicating settings ownership.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
