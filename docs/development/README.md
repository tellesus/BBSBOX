# BBSBOX modular development index

Version 0.1.0 · 2026-10-25 · Tier 1 baseline 0.4.0 · All technical choices/schedules Proposed.

Read [START_HERE](START_HERE.md) and choose a module below. This is a navigational roadmap; a task normally reads only its brief and listed contracts. See [technical overview](../../BBSBOX_TECHNICAL_PROPOSAL.md), [decisions](DECISIONS.md), [coverage](COVERAGE.md), and [work ledger](WORK_LEDGER.md) as lookup documents.

## Core sequence

A valid default order is M00 → M01 → M02 → M03 → M04 → M05 → M06 → M07 → M08 → M09 → M10 → M11 → M12 → M13 → M14 → M15 → M16 → M17 → M18 → M19. This is deliberately serial and reviewable. Independent investigations may overlap, but no module completion bypasses its prerequisites. M00 verifies foundations early; M12 recovery is part of the first local alpha, not postponed to launch.

| Module | Outcome | Gate | Prerequisites | Decision rows |
| --- | --- | --- | --- | --- |
| [M00 — Foundational feasibility and decisions](modules/M00-foundation-spikes.md) | select available test hardware and devices; build a tiny offline page and signed-event round trip. | G0 | None | T01, T02, T03, T04, T05, T06, T07, T08, T14 |
| [M01 — Offline appliance and local access](modules/M01-offline-appliance.md) | offline install and service startup with local status page. | G1 | M00 | T01, T07, T14 |
| [M02 — Durable storage and bounded jobs](modules/M02-storage-jobs.md) | transaction/command idempotency and migration harness. | G1 | M00 | T02, T14 |
| [M03 — Identity, local accounts and permissions](modules/M03-identity-access.md) | local account/login/role boundaries and bootstrap SysOp. | G1 | M00, M02 | T04, T08, T09 |
| [M04 — Signed event kernel and convergence](modules/M04-signed-events.md) | encoder/decoder/signature/authority vectors and size limits. | G1 | M02, M03 | T04, T05 |
| [M05 — Browser shell, navigation and SysOp themes](modules/M05-interface-themes.md) | local landing/login navigation and responsive page skeleton. | G1 | M01, M03 | T03, T07 |
| [M06 — Local boards and private mail](modules/M06-boards-mail.md) | create/read/reply boards with explicit local-only distribution. | G1 | M04, M05 | T05, T09 |
| [M07 — Member directory and chosen profile displays](modules/M07-directory-profiles.md) | account directory, private/public profiles and editable optional fields. | G1 | M03, M05 | T09, T11, T12 |
| [M08 — Chosen feeds, gatherings and invitations](modules/M08-friends-events.md) | chosen public feed and follow/unfollow, with chronological fixtures. | G1 | M04, M05, M07 | T09 |
| [M09 — Local clans and cohorts](modules/M09-clans-cohorts.md) | create groups and expose local directory/profile/roster. | G1 | M03, M05, M07 | T09, T10 |
| [M10 — Local file library](modules/M10-local-files.md) | areas, upload/download and permission model. | G1 | M02, M03, M05 | T02, T09 |
| [M11 — Game host and local multiplayer example](modules/M11-local-games.md) | install/launch a bundled game and scoped identity handshake. | G1 | M02, M03, M05, M07, M09 | T10 |
| [M12 — Offline updates, backup and continuity](modules/M12-maintenance-recovery.md) | consistent backup and isolated restore with node identity/history checks. | G1 | M01, M02, M03, M04 | T08, T12, T14 |
| [M13 — Trusted clients and encrypted forums](modules/M13-encrypted-forums.md) | finalize client trust and MLS decision from M00 evidence; freeze metadata/admission contract. | G2 | M00, M04, M05, M06, M12 | T06, T07, T08 |
| [M14 — Targeted complaints and host suppression](modules/M14-complaints-moderation.md) | report one/multiple targets, preserve explanation/revision and forward case. | G2 | M06, M13 | T09, T13 |
| [M15 — Selected offline peer exchange](modules/M15-selected-federation.md) | two-node public forum/feed exchange and subscription changes. | G3 | M04, M06, M12, M13, M14 | T04, T05, T06 |
| [M16 — Pigeon installation, lumps and credit](modules/M16-pigeons-lump-drops.md) | dedicated courier collect/carry/deliver/return with no running full BBS. | G3 | M07, M15 | T04, T11 |
| [M17 — Local peer trust and exposure reporting](modules/M17-peer-trust.md) | create/inspect a signed report with bounded local evidence. | G3 | M14, M15 | T13 |
| [M18 — Portable characters and Game Passports](modules/M18-game-passports.md) | same-version export/import for the demo game and opt-in profile summary. | G4 | M11, M15, M16 | T10 |
| [M19 — Real mesh and neighborhood pilot](modules/M19-neighborhood-pilot.md) | run local gathering scenario with nontechnical visitors and hosts. | G4 | M08, M09, M10, M12, M14, M16, M17, M18 | T01, T07, T08, T12, T14 |

## Gate evidence

| Gate | Required module evidence | Tier 1 checks / scope |
| --- | --- | --- |
| G0 | M00 completed experiments and decisions; no unproven platform claim | AC01/AC03/AC11/AC14/AC15 feasibility only; record what remains unresolved |
| G1 | M01–M12 integrated, including restore | AC01–AC03, AC08–AC09, AC15, AC20; local portions of AC04/AC10/AC21/AC22. Full file-exchange exclusion is checked in G3. AC17 licensing/source preparation remains an explicit release gate. |
| G2 | M13–M14 integrated with actual client/key paths | Local AC11, AC13–AC14, AC19; no host-blind claim for unsupported client paths |
| G3 | M15–M17 through real interrupted links and carriage | AC04–AC07, AC12, AC18; repeat cross-node AC11/AC13/AC14/AC19 |
| G4 | M18–M19 and predecessor evidence | Full AC10/AC21/AC22; all AC01–AC22 assessed with AC16 explicitly conditional on O11/P20. Complete source/license/materials AC17 before licensed release distribution. |
| G5 | Only independently approved optional slices | Module-specific checks, affected baseline regressions and AC16 if applicable; deferred work remains deferred |

## Optional modules — separate approval, no blanket schedule

G5 is a planning bucket, not a rule that a valuable approved optional experiment must wait until the entire pilot ends. Its actual dependencies and recorded priority determine when it can begin. A deferred D item listed as related background is not activated by an O brief.

| Module | Source proposals | Prerequisites |
| --- | --- | --- |
| [O01 — Search, chat and personal controls](modules/O01-community-utilities.md) | P03 | M06, M08, M13 |
| [O02 — Encrypted cross-node Post Office](modules/O02-post-office.md) | P11 | M13, M15, M16 |
| [O03 — Neighborhood Exchange](modules/O03-neighborhood-exchange.md) | P12 | M07, M08 |
| [O04 — Community newspaper and print editions](modules/O04-community-newspaper.md) | P13 | M06, M07 |
| [O05 — Local knowledge and history wiki](modules/O05-local-wiki.md) | P14 | M06, M07 |
| [O06 — Publisher-controlled Meshcasts](modules/O06-meshcasts.md) | P15 | M08, M15 |
| [O07 — Optional node and forum Atlas](modules/O07-mesh-atlas.md) | P16, P22 | M15, M17 |
| [O08 — Dedicated relay installation](modules/O08-relay-profile.md) | P01, P05 | M15, M17 |
| [O09 — Guest access and kiosk mode](modules/O09-guest-kiosk.md) | P02 | M05, M07, M08, M12 |
| [O10 — Cross-node play and signature dungeons](modules/O10-game-experiments.md) | P18, P19 | M18 |
| [O11 — Issuing-node business game bonuses](modules/O11-business-rewards.md) | P20 | M11, M12 |
| [O12 — Optional recognition and profile/group additions](modules/O12-recognition-extras.md) | P23, P24 | M07, M09, M11, M16 |

## Shared contract ownership

| Contract | Owners | Consumers |
| --- | --- | --- |
| [C01](contracts/C01-storage-runtime.md) | M02, M12 | M01, M02, M03, M04, M05, M07, M10, M11, M12, M19, O01, O08, O09, O11 |
| [C02](contracts/C02-identity-events.md) | M03, M04 | M00, M03, M04, M06, M08, M12, M13, M14, M15, M16, M17, M18, O02, O03, O04, O05, O06, O07, O10 |
| [C03](contracts/C03-exchange-delivery.md) | M15, M16 | M15, M16, M17, M19, O06, O07, O08 |
| [C04](contracts/C04-games.md) | M11, M18 | M09, M11, M18, O10, O11, O12 |
| [C05](contracts/C05-private-content.md) | M07, M13, M14 | M00, M05, M06, M07, M08, M09, M13, M14, O01, O02, O03, O04, O05, O09, O12 |

## Safe modular change procedure

Foundation modules complete their owned interface and local behavior using contract fixtures. A later consumer owns its actual integration and repeated acceptance evidence: for example M16 connects the M07 profile count provider, and M15/M16 repeat M10/M13 checks over real transport. These forward checks are not reverse prerequisites. A contract change lists affected consumers above, increments its version, updates relevant briefs and includes compatibility fixtures. A task needing a dependency detail reads that contract/API first rather than the dependency's entire plan. If the summary is stale against Tier 1, refresh it before implementation. Feature activation, data migration and operator documentation are part of the module handoff, not another hidden project.

No dates, staffing assumptions or numeric capacity promises are made by this first proposal. Track actual work by slices and evidence in WORK_LEDGER; update sequencing when measurements justify it.
