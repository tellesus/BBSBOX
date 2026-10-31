# Locked BBSBOX development roadmap

Version 1.1.0 · 2026-10-25 · Product baseline 0.6.0 · Scope approved under U19; canon clarified under U20. [Historical lock record](../decisions/ADR-0001-roadmap-lock.md).

All named module scopes and T decisions are approved. M00 experiments and M02 A–C have local implementation evidence; G0-local passed. Current module/device/release gates are recorded in WORK_LEDGER. Read [START_HERE](START_HERE.md), one brief and its contracts. Use the live [decisions](DECISIONS.md), derived [coverage](COVERAGE.md) and [ledger](WORK_LEDGER.md) as needed. The [old technical plan](../archive/technical-plan-1.0.0.md) and [earlier review](../archive/review-2026-10-25.md) are historical archives.

## Canon and document ownership

There are two sources of project canon: **the current roadmap and the user’s applicable instructions.** Later explicit user decisions override older roadmap wording and earlier decisions. A request to refine a design, change goals, add a feature or remove one is authority to make that change and synchronize the docs. An exploratory question or a quoted/simulated conversation is not automatically a scope change.

“Roadmap” means this maintained document set, not only the sequence table:

| Canonical component | Owns |
| --- | --- |
| This index and [roadmap.json](roadmap.json) | Sequence, prerequisites, gates and the canonical-document registry |
| [PRODUCT_SPEC](PRODUCT_SPEC.md) | Mission, current R/P/D/N scope, AC acceptance criteria and adopted development lessons |
| [START_HERE](START_HERE.md) | Bounded task workflow and documentation-update obligations |
| [DECISIONS](DECISIONS.md) | Current selected technical decisions; alternatives are reference only |
| The M00–M19/O01–O12 briefs below | Approved behavior, defaults, work slices, owned integrations and completion evidence |
| C01–C05 contracts below | Current shared interfaces and behavior |

[COVERAGE](COVERAGE.md) is a derived index; [WORK_LEDGER](WORK_LEDGER.md) records execution evidence. Neither independently approves scope. Archived reviews, ADR histories, root compatibility redirects, research and other documents are reference/archive, even when their original text says “master,” “locked,” or “approved.” The [archive guide](../archive/README.md) identifies them. Licensing notices remain applicable legal terms, not a competing product roadmap.

Do not resolve a conflict by choosing whichever file sounds more authoritative. Apply the latest applicable user decision and reconcile every affected canonical component. If the intended change is genuinely unclear, keep the affected decision open and ask; continue unrelated work. A reference link alone never promotes material into canon.

Phone tests are scheduled for the G1 integrated local-node alpha and G2 private-client release gates under U22. They remain required there and do not block preceding desktop/local implementation.

## Execution order and dependencies

M00 → M01 → M02 → M03 → M04 → M12 → M05 → M06 → M07 → M08 → M09 → M10 → M11 → M13 → M14 → M15 → M16 → M17 → M18 → M19. Maintenance moves immediately after the durable identity/event foundation. G0-local evidence enables independent local work even if G2 private-client research remains open. A dependency on M00 means G0-local for M01/M02 and full relevant private-client/MLS evidence for M13. Other dependencies require completed provider interfaces; bounded earlier research uses fixtures and claims no integration.

| Module | Gate | Prerequisites for completion | Decisions |
| --- | --- | --- | --- |
| [M00 — Foundational feasibility and decisions](modules/M00-foundation-spikes.md) | G0 | None | T01, T02, T03, T04, T05, T06, T07, T08, T14 |
| [M01 — Offline appliance and local access](modules/M01-offline-appliance.md) | G1 | M00 | T01, T07, T14 |
| [M02 — Durable storage and bounded jobs](modules/M02-storage-jobs.md) | G1 | M00 | T02, T14 |
| [M03 — Identity, local accounts and permissions](modules/M03-identity-access.md) | G1 | M00, M02 | T04, T08, T09 |
| [M04 — Signed event kernel and convergence](modules/M04-signed-events.md) | G1 | M02, M03 | T04, T05 |
| [M12 — Offline updates, backup and continuity](modules/M12-maintenance-recovery.md) | G1 | M01, M02, M03, M04 | T08, T12, T14 |
| [M05 — Browser shell, navigation and SysOp themes](modules/M05-interface-themes.md) | G1 | M01, M03 | T03, T07 |
| [M06 — Local boards and private mail](modules/M06-boards-mail.md) | G1 | M04, M05 | T05, T09 |
| [M07 — Member directory and chosen profile displays](modules/M07-directory-profiles.md) | G1 | M03, M05 | T09, T11, T12 |
| [M08 — Chosen feeds, gatherings and invitations](modules/M08-friends-events.md) | G1 | M04, M05, M07 | T09 |
| [M09 — Local clans and cohorts](modules/M09-clans-cohorts.md) | G1 | M03, M05, M07 | T09, T10 |
| [M10 — Local file library](modules/M10-local-files.md) | G1 | M02, M03, M05, M12 | T02, T09 |
| [M11 — Game host and local multiplayer example](modules/M11-local-games.md) | G1 | M02, M03, M05, M07, M09, M12 | T10 |
| [M13 — Trusted clients and encrypted forums](modules/M13-encrypted-forums.md) | G2 | M00, M04, M05, M06, M08, M12 | T06, T07, T08 |
| [M14 — Targeted complaints and host suppression](modules/M14-complaints-moderation.md) | G2 | M06, M13 | T09, T13 |
| [M15 — Selected offline peer exchange](modules/M15-selected-federation.md) | G3 | M04, M06, M12, M13, M14 | T04, T05, T06 |
| [M16 — Pigeon installation, lumps and credit](modules/M16-pigeons-lump-drops.md) | G3 | M07, M15 | T04, T11 |
| [M17 — Local peer trust and exposure reporting](modules/M17-peer-trust.md) | G3 | M14, M15 | T13 |
| [M18 — Portable characters and Game Passports](modules/M18-game-passports.md) | G4 | M11, M15, M16 | T10 |
| [M19 — Real mesh and neighborhood pilot](modules/M19-neighborhood-pilot.md) | G4 | M08, M09, M10, M12, M14, M16, M17, M18 | T01, T07, T08, T12, T14 |

## Capability gates

| Gate | Exit evidence |
| --- | --- |
| G0-local | M00 desktop access/HTTPS, durable storage, signed-event checks and isolated-runtime IPv4/IPv6 evidence recorded. Enables local development; private-client research and milestone phone checks remain explicit pending work. |
| G1 | Integrated local-node alpha, including public Android Chrome/iOS Safari access/enrollment tests. M01–M12 local integration: AC01–AC03, AC08–AC09, AC15, AC20 and local portions AC04/AC10/AC21/AC22. Source/license materials AC17 are required for each distributed release. |
| G2 | M13/M14 plus M08 encrypted audience adapter: local AC11/AC13/AC14/AC19 and actual independent client/device evidence, including private Android/iOS checks before private-client release. |
| G3 | M15–M17: AC04–AC07, AC12/AC18, actual courier variants and private/case transport checks. Restore replay, scoped streams and suppression cannot bypass policy. |
| G4 | M18/M19 core pilot: full AC10/AC22 and the core game/profile portions of AC21 and all applicable core checks. Approved G5 expansions are honestly listed as pending, including AC16 until O11. |
| G5 | O01–O12 in the order below, each with AC23–AC34 and affected regressions; O12 completes the achievements/progress portion of AC21. O11 must pass AC16 as well. Completing G4 is not completion of the full accepted expansion baseline. |

## Approved expansion sequence — optional activation

O01 → O02 → O03 → O04 → O05 → O06 → O07 → O08 → O09 → O10 → O11 → O12. Every listed scope is approved; do not ask again merely because its ID starts with O. These are independently developed slices with operator opt-in. D01–D07 remain parked. An approved module may be pulled forward once its dependencies pass through a recorded scheduling update.

| Module | Prerequisites | Acceptance |
| --- | --- | --- |
| [O01 — Search, chat and personal controls](modules/O01-community-utilities.md) | M06, M08, M13, M14 | AC23 |
| [O02 — Encrypted cross-node Post Office](modules/O02-post-office.md) | M13, M15, M16 | AC24 |
| [O03 — Neighborhood Exchange](modules/O03-neighborhood-exchange.md) | M06, M07, M08, M15 | AC25 |
| [O04 — Community newspaper and print editions](modules/O04-community-newspaper.md) | M06, M07, M15 | AC26 |
| [O05 — Local knowledge and history wiki](modules/O05-local-wiki.md) | M06, M07, M15 | AC27 |
| [O06 — Publisher-controlled Meshcasts](modules/O06-meshcasts.md) | M08, M15 | AC28 |
| [O07 — Optional node and forum Atlas](modules/O07-mesh-atlas.md) | M15, M17 | AC29 |
| [O08 — Dedicated relay installation](modules/O08-relay-profile.md) | M15, M17 | AC30 |
| [O09 — Guest access and kiosk mode](modules/O09-guest-kiosk.md) | M05, M07, M08, M12 | AC31 |
| [O10 — Cross-node play and signature dungeons](modules/O10-game-experiments.md) | M18 | AC32 |
| [O11 — Issuing-node business game bonuses](modules/O11-business-rewards.md) | M11, M12 | AC33 and AC16 |
| [O12 — Optional recognition and profile/group additions](modules/O12-recognition-extras.md) | M07, M08, M09, M11, M16, O11 | AC34 |

## Shared contract ownership

| Contract | Owners | Consumers |
| --- | --- | --- |
| [C01](contracts/C01-storage-runtime.md) | M02/M12 | M01, M02, M03, M04, M05, M07, M10, M11, M12, M19, O01, O08, O09, O11 |
| [C02](contracts/C02-identity-events.md) | M03/M04 | M00, M03, M04, M06, M08, M12, M13, M14, M15, M16, M17, M18, O03, O04, O05, O06, O07, O10 |
| [C03](contracts/C03-exchange-delivery.md) | M15/M16 | M15, M16, M17, M19, O02, O06, O07, O08 |
| [C04](contracts/C04-games.md) | M11/M18 | M09, M11, M18, O10, O11, O12 |
| [C05](contracts/C05-private-content.md) | M07/M13/M14 | M00, M05, M06, M07, M08, M09, M13, M14, O01, O02, O03, O04, O05, O09, O12 |

## Integration and revision rules

M07/M09 supply profile/membership interfaces; M08/M11/M16/M18 own live consumers. M12 supplies maintenance/checkpoint APIs; M10/M11/M16 bind their real data and redo restore checks. M13/M14 complete local behavior; M15/M16 own real disconnected private/case integration. This prevents hidden completion cycles.

The [machine-readable graph](roadmap.json) is the source for ordering/ownership validation; brief prerequisites and this index must agree. Contract changes identify consumers, bump the affected version and update compatibility fixtures. Each gate records actual results and pending capabilities. No dates or numeric hardware promises are implied by the lock.

## Change control

Every design refinement, goal change, feature addition/removal, scope/status change or technical replacement includes the corresponding canonical documentation edits in the same work and commit. Do not finish with a chat-only decision, an ADR-only amendment, or obsolete instructions still active.

1. Record the user instruction or existing delegated authority and the affected IDs. Do not request approval again for an already authorized change.
2. Update the owning product requirements, module briefs, technical decisions and shared contracts as applicable. Remove superseded instructions; retain retired IDs as short tombstones with the change/replacement reference.
3. Update acceptance checks, dependencies/gates, graph, coverage and affected consumers. If a decision changes while implementation is underway, update canon before continuing dependent work. Adjust the work ledger without turning design approval into test evidence.
4. Update document versions and the product change record. Archive superseded rationale with a dated non-canonical label; future ADRs may explain a decision but cannot substitute for updating live canon.
5. Run `python tools/validate_roadmap.py`, check links and review wording for contradictions. Complete the change only when affected canonical documents agree. Publish the documentation with the authorized implementation/planning change.

Routine implementation details that do not change a documented contract or decision need only normal implementation notes. When they refine the design or change expected behavior, the same documentation rule applies. Product and protocol versions remain independent of document versions.
