# M15 — Selected offline peer exchange

Version 0.1.0 · 2026-10-25 · Tier 2 proposal · Tier 1 baseline 0.4.0.

**State:** Planned capability; implementation choices proposed. No implementation or passed checks claimed.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C02](../contracts/C02-identity-events.md), [C03](../contracts/C03-exchange-delivery.md).
Look up only decision rows T04, T05, T06 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R02, R08, R09, R12, R13, R15, P06, P21, P22, AC04, AC05, AC07, AC11, AC13, AC14. **Target gate:** G3. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M04](../modules/M04-signed-events.md), [M06](../modules/M06-boards-mail.md), [M12](../modules/M12-maintenance-recovery.md), [M13](../modules/M13-encrypted-forums.md), [M14](../modules/M14-complaints-moderation.md).

## Product outcome and boundaries

Exchange only selected forums/feeds between independently operated nodes. Local-only content and file libraries remain local. Pigeons will reuse the same importer.

## Recommended implementation

Use a transport adapter over explicit offline IP peer links, authenticated by pinned node credentials introduced with fingerprints/QR. HTTPS/mTLS or an equivalent authenticated local channel is proposed; message signatures remain authoritative across carriage. Exchange versioned inventory/gap requests and bounded chunks. Both sender and receiver enforce subscription policy. A peer can decline a module/version while other compatible streams continue.

## Ownership and interfaces

Proposed crates/sync/; peer sessions, subscriptions, inventories, persistent inbox/outbox and gap requests. Does not own radio routing, event reducers or group keys.

Paths above are proposed implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: two-node public forum/feed exchange and subscription changes.
2. B: resumable/reordered chunks, missing dependencies and mixed versions.
3. C: encrypted forum/case traffic plus local responsiveness under sustained ingress.

## Completion evidence

- AC05 selects one shared forum while two others stay local/unselected.
- AC07 and private-group fixtures converge after reconnection without requiring internet or synchronized clocks.
- Disabling a peer stops future exchange per documented retention policy while local content stays usable.

## Decisions before completion

Choose exact authenticated link setup and subscription withdrawal retention. Initial single-link IP adapter must work before adding radio-specific integrations.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). A passing fixture is evidence for this module, not approval of new product scope. See [coverage](../COVERAGE.md) when changing source IDs.
