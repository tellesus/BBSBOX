# O11 — Issuing-node business game bonuses

Version 0.1.0 · 2026-10-25 · Tier 2 proposal · Tier 1 baseline 0.4.0.

**State:** Optional candidate; product approval required. No implementation or passed checks claimed.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C01](../contracts/C01-storage-runtime.md), [C04](../contracts/C04-games.md).
Look up only decision rows T10 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** P20, R22, AC16. **Target gate:** G5. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M11](../modules/M11-local-games.md), [M12](../modules/M12-maintenance-recovery.md).

## Product outcome and boundaries

Conditional receipt-code game bonuses that support the local host business. Purchase is voluntary and changes no social reach. Redemption works only at the issuer.

## Recommended implementation

Use random one-use codes stored as hashes at the issuing node, with game/reward references and explicit operator issuance. Atomically redeem into a durable game reward command; both code redemption and reward application are idempotent. Start with manual code issuance; no POS connection or internet payment integration. Game-earned business coupons can be a later independently approved slice.

## Ownership and interfaces

Proposed crates/business-rewards/; issuance, redemption ledger and reward-delivery outbox. Games own reward effects; host owns available offers.

Paths above are proposed implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: operator issues a code and user redeems locally.
2. B: reward delivery/retry and wrong-node rejection.
3. C: optional reverse game-earned coupon workflow.

## Completion evidence

- AC16 includes duplicate/concurrent redemption and a crash before game acknowledgment.
- Another BBS cannot redeem the code even when it recognizes the character.
- Users without a purchase retain ordinary participation/feed/invitation reach.

## Decisions before completion

Choose one supported game/reward, expiry/correction rules and whether reverse coupons are wanted.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). A passing fixture is evidence for this module, not approval of new product scope. See [coverage](../COVERAGE.md) when changing source IDs.
