# BBSBOX development — bounded task context

Version 1.0.0 · 2026-10-25 · Locked roadmap · Tier 1 0.5.0 · Authority U19 / ADR-0001.

## Work one module slice at a time

Read this file, one module brief and its listed contracts. Look up only its T decision rows. The index/coverage/review documents are lookup tools, not required context on every task. A normal reading packet targets 2,500 words; exact byte schemas and code are additional task-specific implementation context.

P01–P24, T01–T14 and M00–M19/O01–O12 are approved within the recorded scopes. Use their locked defaults without asking for permission again. Optional activation is an operator/user choice. D01–D07 and unnamed additions remain outside this baseline. Hardware compatibility, supported versions and capacity are evidence to collect, not facts established by approval.

Before implementation, verify the selected slice's prerequisites and contract versions, then record source IDs, included behavior, owned data, fixtures and exit evidence. M00 establishes G0-local access/storage/event evidence; unfinished private-client/MLS evidence blocks G2 only. Earlier modules complete their interfaces with fixtures; later consumers own actual integration and regression checks. A task cannot claim integration from mocks.

## Always-applicable boundaries

- Neighborhood use; useful standalone nodes; modern browser UI; vibrant colors on black by default and SysOp customization. Infrastructure never seeks internet; visitors may have independent connectivity.
- Local file libraries stay local; selected federation only. Signatures and action authority differ. Streams are scoped by device AND resource/audience. Old packages can contain new valid events.
- Person, node, forum and courier powers are distinct. Owners moderate encrypted forums. Hosts may suppress exact referenced posts locally without plaintext. Delayed revocation is accepted; no global posting halt.
- Chosen chronological posts/events; local clans/cohorts. Passport copy/import needs owner proof and configured GM acceptance. Business codes redeem only at issuer; purchases never alter social reach.
- Lump Drops count qualifying receiving deliveries once, attributed to the delivering person; private display remains separate from recording. O12 adds approved manual rewards later.
- All local accounts remain in the directory; optional profile/game fields obey privacy. AGPL-3.0-or-later covers project software/tools, CC-BY-SA-4.0 design prose; third-party and user content retain their own terms.

## Handoff and changes

Record module/slice, baseline, ADR, contract/schema versions, commit, actual tests/observations, limitations and next slice in [WORK_LEDGER](WORK_LEDGER.md). Current implementation state is Unstarted. This lock records design review, not runtime success.

Use [Tier 1](../../BBSBOX_MASTER_SPEC.md) for targeted scope questions. Material changes get a successor ADR and affected-consumer updates; newly added product scope requires a Tier 1 change. Routine compatible implementation details and dependency pinning are authorized. A selected technology failing its gate calls for an evidence-backed replacement; never silently weaken the privacy/offline requirement.

Task record: `Module/slice; baseline; source IDs; ADR; included behavior; boundaries; owned files/data; contracts; prerequisite evidence; fixtures; result/commit; next action.`
