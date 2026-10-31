# BBSBOX development — bounded task context

Version 1.1.0 · 2026-10-25 · Locked roadmap · Product baseline 0.6.0 · U19 scope / U20 canon / local-first testing under U22.

The [roadmap](README.md#canon-and-document-ownership) and the user’s latest applicable instructions are the two sources of canon; user instructions take precedence. Root specification/proposal redirects, archived reviews and ADR histories are reference only. An exploratory question is not automatically a feature decision.

## Work one module slice at a time

Read this file, one module brief and its listed contracts. Look up only its T decision rows. The index/coverage/review documents are lookup tools, not required context on every task. A normal reading packet targets 2,500 words; exact byte schemas and code are additional task-specific implementation context.

P01–P24, T01–T14 and M00–M19/O01–O12 are approved within the recorded scopes. Use their locked defaults without asking for permission again. Optional activation is an operator/user choice. D01–D07 and unnamed additions remain outside this baseline. Hardware compatibility, supported versions and capacity require evidence. Under the user’s current instruction, phone tests occur at G1 public-alpha/G2 private-client milestones; local work continues beforehand.

Before implementation, verify the selected slice's prerequisites and contract versions, then record source IDs, included behavior, owned data, fixtures and exit evidence. M00 establishes G0-local access/storage/event evidence; unfinished private-client/MLS evidence blocks G2 only. Earlier modules complete their interfaces with fixtures; later consumers own actual integration and regression checks. A task cannot claim integration from mocks.

## Always-applicable boundaries

- Neighborhood use; useful standalone nodes; modern browser UI; vibrant colors on black by default and SysOp customization. Infrastructure never seeks internet; visitors may have independent connectivity.
- Local file libraries stay local; selected federation only. Signatures and action authority differ. Streams are scoped by device AND resource/audience. Old packages can contain new valid events.
- Person, node, forum and courier powers are distinct. Owners moderate encrypted forums. Hosts may suppress exact referenced posts locally without plaintext. Delayed revocation is accepted; no global posting halt.
- Chosen chronological posts/events; local clans/cohorts. Passport copy/import needs owner proof and configured GM acceptance. Business codes redeem only at issuer; purchases never alter social reach.
- Lump Drops count qualifying receiving deliveries once, attributed to the delivering person; private display remains separate from recording. O12 adds approved manual rewards later.
- All local accounts remain in the directory; optional profile/game fields obey privacy. AGPL-3.0-or-later covers project software/tools, CC-BY-SA-4.0 design prose; third-party and user content retain their own terms.

## Handoff and changes

Record module/slice, baseline, ADR, contract/schema versions, commit, tests, limitations and next slice in [WORK_LEDGER](WORK_LEDGER.md). G0-local passed; M02/M03 providers Complete; M00/M04 In progress. The ledger distinguishes local evidence from device/release acceptance.

For every design refinement, goal change or feature addition/removal, update affected product requirements, module briefs, decisions/contracts, acceptance checks and graph/coverage in the same change. Follow [change control](README.md#change-control); record the authority and version/change entry, retire obsolete instructions, and run the roadmap validator before completion. An ADR or chat summary alone is insufficient. Already authorized changes need no repeated permission. Routine compatible implementation details remain authorized.

Task record: `Module/slice; baseline; source IDs; ADR; included behavior; boundaries; owned files/data; contracts; prerequisite evidence; fixtures; result/commit; next action.`
