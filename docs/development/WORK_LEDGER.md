# Development work ledger

Version 0.1.0 · 2026-10-25 · Documentation baseline only. All modules below are unstarted; no tests or deployment results are claimed.

## State definitions

Unstarted → Investigating → Ready (decisions/prerequisites established) → Implementing → Validating → Complete. Optional modules also require a recorded scope approval. A dependency can provide a mock for experimentation but only real integration evidence satisfies completion. Blocked means a named missing decision/evidence; record independent work that can still proceed.

| Module | State | Completed slices | Decision / evidence / commit | Next action |
| --- | --- | --- | --- | --- |
| [M00](modules/M00-foundation-spikes.md) | Unstarted | None | None | Check prerequisites; begin slice A |
| [M01](modules/M01-offline-appliance.md) | Unstarted | None | None | Check prerequisites; begin slice A |
| [M02](modules/M02-storage-jobs.md) | Unstarted | None | None | Check prerequisites; begin slice A |
| [M03](modules/M03-identity-access.md) | Unstarted | None | None | Check prerequisites; begin slice A |
| [M04](modules/M04-signed-events.md) | Unstarted | None | None | Check prerequisites; begin slice A |
| [M05](modules/M05-interface-themes.md) | Unstarted | None | None | Check prerequisites; begin slice A |
| [M06](modules/M06-boards-mail.md) | Unstarted | None | None | Check prerequisites; begin slice A |
| [M07](modules/M07-directory-profiles.md) | Unstarted | None | None | Check prerequisites; begin slice A |
| [M08](modules/M08-friends-events.md) | Unstarted | None | None | Check prerequisites; begin slice A |
| [M09](modules/M09-clans-cohorts.md) | Unstarted | None | None | Check prerequisites; begin slice A |
| [M10](modules/M10-local-files.md) | Unstarted | None | None | Check prerequisites; begin slice A |
| [M11](modules/M11-local-games.md) | Unstarted | None | None | Check prerequisites; begin slice A |
| [M12](modules/M12-maintenance-recovery.md) | Unstarted | None | None | Check prerequisites; begin slice A |
| [M13](modules/M13-encrypted-forums.md) | Unstarted | None | None | Check prerequisites; begin slice A |
| [M14](modules/M14-complaints-moderation.md) | Unstarted | None | None | Check prerequisites; begin slice A |
| [M15](modules/M15-selected-federation.md) | Unstarted | None | None | Check prerequisites; begin slice A |
| [M16](modules/M16-pigeons-lump-drops.md) | Unstarted | None | None | Check prerequisites; begin slice A |
| [M17](modules/M17-peer-trust.md) | Unstarted | None | None | Check prerequisites; begin slice A |
| [M18](modules/M18-game-passports.md) | Unstarted | None | None | Check prerequisites; begin slice A |
| [M19](modules/M19-neighborhood-pilot.md) | Unstarted | None | None | Check prerequisites; begin slice A |
| [O01](modules/O01-community-utilities.md) | Unstarted | None | None | Review scope before selecting a slice |
| [O02](modules/O02-post-office.md) | Unstarted | None | None | Review scope before selecting a slice |
| [O03](modules/O03-neighborhood-exchange.md) | Unstarted | None | None | Review scope before selecting a slice |
| [O04](modules/O04-community-newspaper.md) | Unstarted | None | None | Review scope before selecting a slice |
| [O05](modules/O05-local-wiki.md) | Unstarted | None | None | Review scope before selecting a slice |
| [O06](modules/O06-meshcasts.md) | Unstarted | None | None | Review scope before selecting a slice |
| [O07](modules/O07-mesh-atlas.md) | Unstarted | None | None | Review scope before selecting a slice |
| [O08](modules/O08-relay-profile.md) | Unstarted | None | None | Review scope before selecting a slice |
| [O09](modules/O09-guest-kiosk.md) | Unstarted | None | None | Review scope before selecting a slice |
| [O10](modules/O10-game-experiments.md) | Unstarted | None | None | Review scope before selecting a slice |
| [O11](modules/O11-business-rewards.md) | Unstarted | None | None | Review scope before selecting a slice |
| [O12](modules/O12-recognition-extras.md) | Unstarted | None | None | Review scope before selecting a slice |

## Task handoff record

For each completed work item append: date; module/slice; Tier 1 baseline; accepted ADRs; contract/schema versions; source commit; exact tests/observations with evidence paths; outcomes; known limits; migration/rollback notes; next bounded task. Update state only from evidence. An implementation document can be Ready while its feature has never shipped.

## Release capability record

Before a release add its artifact/signature, source/license decision, supported hardware/devices, delivered modules/slices, pending requirements, AC results, operational runbook and restore evidence. Document incompatible versions and unsupported client modes. Never infer completion from this planned roadmap.
