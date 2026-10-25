# M13 — Trusted clients and encrypted forums

Version 1.0.0 · 2026-10-25 · Locked development baseline · Tier 1 baseline 0.5.0.

**State:** Approved for implementation under U19; unstarted. Runtime and hardware evidence remains pending.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C02](../contracts/C02-identity-events.md), [C05](../contracts/C05-private-content.md).
Look up only decision rows T06, T07, T08 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R13, R15, R17, P07, P09, AC11, AC14, AC15. **Target gate:** G2. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M00](../modules/M00-foundation-spikes.md), [M04](../modules/M04-signed-events.md), [M05](../modules/M05-interface-themes.md), [M06](../modules/M06-boards-mail.md), [M08](../modules/M08-friends-events.md), [M12](../modules/M12-maintenance-recovery.md).

## Product outcome and boundaries

Members decrypt private forum content; hosting/carriage does not grant plaintext. Disconnected stale-credential access is accepted; no network-wide posting halt.

## Locked implementation

Implement the selected MLS/OpenMLS path with shared Rust/native crypto and a separately trusted signed Tauri client bundle. Evaluate WASM where the client boundary needs it; library selection does not itself establish device support. Keep MLS key packages/epochs on member clients and opaque payloads on hosts. Use owner-authorized serialized governance commits with delegated moderator actions; ordinary posting uses the client's locally known valid epoch. Handle stale epochs, commit branches and retention according to measured library behavior, not improvised cryptography. Exact outer admission proofs must let a host validate a new encrypted event without learning its content.

## Ownership and interfaces

Planned crates/private-forum/ and client/crypto/; encrypted event kinds, public authority proofs, client group/key state, invitations and encrypted client backups. Hosts own ciphertext indexes only.

Paths above are planned implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: verify the selected client trust and MLS path against M00 evidence; freeze metadata/admission contract.
2. B: create/invite/post/read/remove with two clients and no host keys.
3. C: prolonged disconnection, concurrent governance, old-epoch messages and restore/rejoin.
4. D: integrate M08 confirmed-friend audiences with encrypted sharing; repeat visibility tests without changing public following.
5. E: owner-authorized historical access: an authorized member explicitly republishes selected readable history as an encrypted archive for the selected current-member audience through the established group encryption path. Preserve original signed provenance and mark it as a historical copy. Do not grant old epoch keys automatically or promise recovery of history no remaining client can decrypt. Test new-member denial by default, explicit grant, and removal affecting future access while retained past copies remain possible.

Later integration is owned by M15/M16: repeat these checks over real transport before federation is called private. This is not a prerequisite for completing the encrypted-local module.

## Completion evidence

- AC11 includes a hosting-only SysOp and carrier that cannot decrypt or replace trusted client code undetected under the selected model.
- AC14 preserves accepted stale-state behavior without rejecting valid unrelated offline work.
- A fresh relevant encrypted event is recognizable at its authenticated envelope boundary for M16; unknown authority remains pending.

## Locked defaults and verification

MLS/OpenMLS is selected for test implementation, with Tauri bundled UI/crypto as the first independent private-client path. Owners serialize governance changes; delegated moderator proposals queue until authorized commit, while ordinary offline posting continues. Start retained-epoch tests at 32 epochs, configurable; longer absence uses explicit rejoin/history recovery, never silent decryption guarantees. Outer admission proves signer/resource/epoch and role without plaintext. M13 also binds M08 friend audiences after M08 base completion. G2 cannot pass without actual supported-device and stale-state evidence.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved; genuinely new scope still follows change control. See [coverage](../COVERAGE.md) when changing source IDs.
