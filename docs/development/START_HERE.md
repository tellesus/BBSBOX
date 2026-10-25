# BBSBOX development — bounded task context

Version 0.1.0 · 2026-10-25 · Tier 2 planning proposal · Tier 1 baseline 0.4.0.

## Use one module at a time

1. Choose a module in [the index](README.md). Read its brief and only the contracts listed in its context packet. Normally this file + one brief + one or two contracts is enough; aim for under 2,500 words of mandatory design context. The index/coverage map are lookup tools, not mandatory reading for every change.
2. Check the referenced decision rows in [DECISIONS](DECISIONS.md), the module's prerequisite completion records, and the current code. A proposal authorizes investigation, not an assertion that its unresolved choice has already been made.
3. Create one bounded task with module ID, baseline, approved decision references, exact included slice, touched interfaces, fixtures, and exit evidence. Module briefs list sequential slices; completing one slice does not complete the module.
4. Implement inside the proposed ownership boundary. Use mocks/fixtures for dependencies while prototyping; record that integration remains outstanding. No production completion claim based only on mocks.
5. Record result, commit, tests/observations, schema/contract versions, remaining work, and updated module state in the [work ledger](WORK_LEDGER.md). Update affected contract consumers and the coverage map if an interface changes.

## Always-applicable constraints

- R01–R06: neighborhood purpose; each node useful alone; modern browser UI; vibrant colors on black by default; SysOp customization; modest hardware; infrastructure never seeks the internet. Client devices may independently have internet.
- R08–R17: file libraries stay local; federation is selected; signatures and permissions are distinct; older delivery can supply missing relevant events; node, user, forum and courier authority differ. Hosts/carriers do not acquire private forum keys. Owner-led moderation and reference-based local suppression coexist. Delayed revocation is accepted; no global posting halt to simulate instant revocation.
- R18–R27: chosen chronological posts/events; game portability only where supported, with configured GM approval; optional business rewards redeem only at issuer; license TBD with reciprocal intent; Lump Drops count qualifying receiving deliveries once and can be hidden; every local account is in the directory; clans/cohorts remain local.
- Product scope is [Tier 1](../../BBSBOX_MASTER_SPEC.md). All decisions and schedules in this package are **Proposed**, unless a later recorded approval changes that status. P and D features remain optional/deferred. No implementation exists in this documentation baseline.

## Scope and contract escalation

The brief restates its relevant product constraints. Consult the specific Tier 1 section when a question exceeds that summary; load the whole master only for a scope audit. A newly discovered product change gets an explicit Tier 1 change record. A compatible implementation detail stays in its module/ADR. Changes to a shared contract identify affected consumers and version/upgrade behavior in the same proposal.

## Work item template

`Module/slice; baseline; source IDs; accepted ADRs; included behavior; exclusions; owned files/data; consumed contract revisions; prerequisite evidence; fixtures; acceptance; result/commit; next slice.`

Review recommendations on their merits. Do not copy Mastodon features, protocols, staffing scale, or internet services merely because they appear in research.
