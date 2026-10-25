# M10 — Local file library

Version 0.1.0 · 2026-10-25 · Tier 2 proposal · Tier 1 baseline 0.4.0.

**State:** Planned capability; implementation choices proposed. No implementation or passed checks claimed.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C01](../contracts/C01-storage-runtime.md).
Look up only decision rows T02, T09 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R07, R08, AC01, AC04. **Target gate:** G1. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M02](../modules/M02-storage-jobs.md), [M03](../modules/M03-identity-access.md), [M05](../modules/M05-interface-themes.md).

## Product outcome and boundaries

Provide local uploads/downloads under local permissions. Libraries remain outside mesh/Pigeon synchronization.

## Recommended implementation

Store bytes by content hash and keep owner/name/media type/size/access metadata in SQLite. Stream uploads to temporary storage with bounded quotas, finalize bytes, then expose metadata. Serve downloads with safe disposition and local authorization. Separate release-maintenance artifacts from user file entries. Deletion/retention must respect active backup references.

## Ownership and interfaces

Proposed crates/files/; file-area metadata and blob lifecycle through C01. No federated event kind advertises library bytes for automatic retrieval.

Paths above are proposed implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: areas, upload/download and permission model.
2. B: interrupted upload cleanup, quotas and deletion.
3. C: prove the library exposes no automatic export source using an inventory fixture. M15/M16 own the later real exchange checks; they do not block completion of the local file library.

## Completion evidence

- AC04 local portion: local download works and library data is absent from the export-source fixture. M15/M16 subsequently prove it never appears in a real lump or sync offer.
- Disk exhaustion cannot produce a successful metadata-only upload.
- Restore preserves referenced files and their access settings.

## Decisions before completion

Choose area creation/upload limits and permitted preview formats; general post attachments require their own decision.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). A passing fixture is evidence for this module, not approval of new product scope. See [coverage](../COVERAGE.md) when changing source IDs.
