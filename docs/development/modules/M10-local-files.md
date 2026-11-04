# M10 — Local file library

Brief revision 1.1.2 · 2026-11-04 · Roadmap baseline 1.1.0 · Product baseline 0.6.0.

**State:** Complete for the software provider. A–C pass locally and in signed Debian CI.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C01](../contracts/C01-storage-runtime.md).
Look up only decision rows T02, T09 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R07, R08, AC01, AC04. **Target gate:** G1. Scope summaries below preserve these source IDs; reconcile conflicts with the current product requirements and user instructions.

**Prerequisites:** [M02](M02-storage-jobs.md), [M03](M03-identity-access.md), [M05](M05-interface-themes.md), [M12](M12-maintenance-recovery.md).

## Product outcome and boundaries

Provide local uploads/downloads under local permissions. Libraries remain outside mesh/Pigeon synchronization.

## Locked implementation

Store bytes by content hash and keep owner/name/media type/size/access metadata in SQLite. Stream uploads to temporary storage with bounded quotas, finalize bytes, then expose metadata. Serve downloads with safe disposition and local authorization. Separate release-maintenance artifacts from user file entries. Deletion/retention must respect active backup references.

## Ownership and interfaces

`crates/files/` owns file areas and metadata. C01 owns storage and blob references. The library publishes no events for exchange.

Use the listed contracts. Disabling a module retains its data.

## Sequential work slices

1. A: areas, upload/download and permission model.
2. B: interrupted upload cleanup, quotas and deletion.
3. C: prove the library exposes no automatic export source using an inventory fixture. M15/M16 own the later real exchange checks; they do not block completion of the local file library.

## Completion evidence

- AC04 local portion: local download works and library data is absent from the export-source fixture. M15/M16 subsequently prove it never appears in a real lump or sync offer.
- Disk exhaustion cannot produce a successful metadata-only upload.
- Restore preserves referenced files and their access settings.

## Locked defaults and verification

SysOps set each area's download, upload and quota policy. A new area defaults to member downloads, SysOp uploads and a 64 MiB quota. A file is capped at 16 MiB. Uploads stream to disk before metadata is published. Downloads are attachments. M12 owns restore integration. M15/M16 verify exclusion from actual exchanges.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
