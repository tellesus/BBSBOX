# M11 — Game host and local multiplayer example

Brief revision 1.1.17 · Roadmap baseline 1.1.0 · 2026-11-06 · Locked scope · Product baseline 0.6.0.

**State:** Complete for the software provider. Local browser/recovery checks and signed Debian installation, enablement, complete replacement recovery and coordinated game/core update/rollback pass. G1 physical/phone acceptance remains pending.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C01](../contracts/C01-storage-runtime.md), [C04](../contracts/C04-games.md).
Look up only decision rows T10 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R07, R20, R21, R26, R27, P17, AC01, AC10, AC21, AC22. **Target gate:** G1. Scope summaries below preserve these source IDs; reconcile conflicts with the current product requirements and user instructions.

**Prerequisites:** [M02](M02-storage-jobs.md), [M03](M03-identity-access.md), [M05](M05-interface-themes.md), [M07](M07-directory-profiles.md), [M09](M09-clans-cohorts.md), [M12](M12-maintenance-recovery.md).

## Product outcome and boundaries

Make games part of the first useful node with one small local game and multiplayer example. Define passport/profile/group hooks now; actual cross-node portability arrives in M18.

## Locked implementation

Use locally installed HTML/TypeScript game assets and an isolated worker with JSON HTTP/WebSocket messages. Start with a simple turn-based two-player demonstration; no particular fictional dungeon or game title is committed. The game owns rules/state and publishes optional character summaries. Core authenticates players and supplies scoped sessions plus local memberships. Avoid a general plugin marketplace or arbitrary code-loader in the core.

## Ownership and interfaces

`crates/game-host/` owns manifests, launch tickets and framed worker requests. `games/demo/` owns the demo's rules, SQLite state and command receipts.

The offline release build signs the bundled worker and both of its service files. The fresh installer provisions a dedicated worker account and installs the verified service files inactive. The signed service requires appliance enablement before it accepts a game origin.

The offline network preparer can reserve `bbsbox-game-<node suffix>.home.arpa` in local DNS and the node certificate. An explicit `--enable-game` during fresh signed network preparation connects that hostname to the isolated worker socket at the next boot.

`backup-game-node` captures the stopped node through M12's [game-node snapshot provider](../../../crates/maintenance/README.md#game-node-snapshot-profile). The signed appliance provider uses the separate worker account and socket for an existing save. `restore-game-node` verifies and stages a matching demo checkpoint for recovery. Foundation backup refuses retained game state.

`restore-game-node-and-record` binds the operator's retired or lost source decision to the verified game-node archive. Fresh signed recovery verifies the game save under its dedicated account, then compares the placed save and archived inventory before releasing the staged writer. The replacement boots on loopback before offline network preparation.

Installed updates select the complete game-node snapshot whenever a game origin or retained save exists. This provider requires identical signed worker and service files across the update. Failed updates and recorded rollback restore the matching game/core pair under maintenance control. Rollback retains the newer complete encrypted snapshot and displaced saves for separate reconciliation. Changing the worker or its save format requires a subsequent compatible migration provider.

Use the listed contracts. Disabling a game retains its state.

## Sequential work slices

1. A: install/launch a bundled game and scoped identity handshake.
2. B: two local players, reconnect and save/restart.
3. C: opt-in profile character display and local group lookup; define adapter stubs for M18.

## Completion evidence

- Two users play through the browser while a third uses local boards.
- A command retry changes game state once.
- Logging out invalidates outstanding tickets and sessions from that sign-in.
- Profile settings hide game/character summaries; local groups remain local.

[CI 42310888214](https://github.com/tellesus/BBSBOX/actions/runs/42310888214) passes all four jobs through `a29d174`. It verifies isolation, complete replacement, update, further play, matching-pair rollback and retained newer play. Edge gameplay and third-user board access pass locally. G1 phones remain pending.

## Locked defaults and verification

Build a small turn-based cooperative dungeon demo with persistent characters and two local players as a test fixture, not a promised game brand. Game workers run separately with per-game state and scoped command IDs. A sandboxed game origin never receives core cookies or client keys; bootstrap game-scoped tokens through the host. Supply profile/group hooks now; M18 adds snapshot portability. C01 defines game checkpoint participation.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
