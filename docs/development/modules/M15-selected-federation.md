# M15 — Selected offline peer exchange

Version 1.1.0 · 2026-10-25 · Locked development baseline · Product baseline 0.6.0.

**State:** Approved for implementation under U19; unstarted. Runtime and hardware evidence remains pending.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C02](../contracts/C02-identity-events.md), [C03](../contracts/C03-exchange-delivery.md).
Look up only decision rows T04, T05, T06 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R02, R08, R09, R12, R13, R15, P06, P21, P22, AC04, AC05, AC07, AC11, AC13, AC14. **Target gate:** G3. Scope summaries below preserve these source IDs; reconcile conflicts with the current product requirements and user instructions.

**Prerequisites:** [M04](M04-signed-events.md), [M06](M06-boards-mail.md), [M12](M12-maintenance-recovery.md), [M13](M13-encrypted-forums.md), [M14](M14-complaints-moderation.md).

## Product outcome and boundaries

Exchange only selected forums/feeds between independently operated nodes. Local-only content and file libraries remain local. Pigeons will reuse the same importer.

## Locked implementation

Use a transport adapter over explicit offline IP peer links, authenticated by pinned node credentials introduced with fingerprints/QR. Use HTTPS with mutually authenticated pinned node credentials; message signatures remain authoritative across carriage. Exchange versioned inventory/gap requests and bounded chunks. Both sender and receiver enforce subscription policy. A peer can decline a module/version while other compatible streams continue.

## Ownership and interfaces

Planned crates/sync/; peer sessions, subscriptions, inventories, persistent inbox/outbox and gap requests. Does not own radio routing, event reducers or group keys.

Paths above are planned implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: two-node public forum/feed exchange and subscription changes.
2. B: resumable/reordered chunks, missing dependencies and mixed versions.
3. C: encrypted forum/case traffic plus local responsiveness under sustained ingress.

## Completion evidence

- AC05 selects one shared forum while two others stay local/unselected.
- AC07 and private-group fixtures converge after reconnection without requiring internet or synchronized clocks.
- Disabling a peer stops future exchange per documented retention policy while local content stays usable.

## Locked defaults and verification

Use TLS with pinned node identity credentials for peer exchange; local operator introductions bind fingerprints, with separate event signatures. No public CA/registry requirement. Unsubscribe stops future offers but retains already accepted local history until explicit local removal. Handshake advertises core/module versions and subscription scopes. Public exchange slices can be tested before M13/M14; complete M15 still requires private/case integration evidence.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
