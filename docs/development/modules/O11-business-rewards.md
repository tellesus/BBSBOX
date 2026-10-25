# O11 — Issuing-node business game bonuses

Version 1.0.0 · 2026-10-25 · Locked development baseline · Tier 1 baseline 0.5.0.

**State:** Approved for implementation under U19; unstarted. Runtime and hardware evidence remains pending.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C01](../contracts/C01-storage-runtime.md), [C04](../contracts/C04-games.md).
Look up only decision rows T10 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R28, P20, R22, AC16. **Target gate:** G5. **Expansion acceptance:** AC33. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M11](../modules/M11-local-games.md), [M12](../modules/M12-maintenance-recovery.md).

## Product outcome and boundaries

Approved receipt-code game bonuses that support the local host business. Purchase is voluntary and changes no social reach. Redemption works only at the issuer.

## Locked implementation

Use random one-use codes stored as hashes at the issuing node, with game/reward references and explicit operator issuance. Atomically redeem into a durable game reward command; both code redemption and reward application are idempotent. Start with manual code issuance; no POS connection or internet payment integration. Game-earned business coupons can be a later approved slice.

## Ownership and interfaces

Planned crates/business-rewards/; issuance, redemption ledger and reward-delivery outbox. Games own reward effects; host owns available offers.

Paths above are planned implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: operator issues a code and user redeems locally.
2. B: reward delivery/retry and wrong-node rejection.
3. C: optional reverse game-earned coupon workflow.

## Completion evidence

- AC16 includes duplicate/concurrent redemption and a crash before game acknowledgment.
- Another BBS cannot redeem the code even when it recognizes the character.
- Users without a purchase retain ordinary participation/feed/invitation reach.

## Locked defaults and verification

Manual one-use receipt-code issuance and reverse game-earned business coupons are both approved, separate slices. Codes bind issuer/game/reward and expiry if set; defaults have no automatic expiry. Redemptions and reward outbox use durable IDs, and the game worker deduplicates independently. Manual void/correction is recorded. No POS or payment service; only issuer redeems.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved; genuinely new scope still follows change control. See [coverage](../COVERAGE.md) when changing source IDs.
