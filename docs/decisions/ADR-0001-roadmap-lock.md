# ADR-0001 — Reviewed test lock and implementation roadmap

Status: Accepted for implementation/testing · 2026-10-25 · Authority: Michael U19.
Tier 1: 0.5.0 · Roadmap/contracts: 1.0.0 · Supersedes proposal baseline 0.1.0.

## Decision and authority

The user authorized best-judgment test lock of all proposals, senior review/corrections, final roadmap lock and GitHub publication. Accept the defined P01–P24 feature scopes, M00–M19/O01–O12 sequence and T01–T14 selections. Optional modules are approved work with optional activation. Deferred D01–D07 and unnamed future embellishments are not silently promoted.

Select Rust/Axum/Tokio on Debian, SQLite WAL/rusqlite, static SvelteKit/TypeScript, deterministic CBOR/COSE/Ed25519/SHA-256 and scope-specific signed streams. Select MLS/OpenMLS and Tauri as the private group/client test path. Their release gates require actual disconnected-state and supported-device evidence. Select per-person roots/device credentials, receiver-local Lump Drops, explicit game copy/import branches, manual peer decisions, signed offline updates, AGPL-3.0-or-later software and CC-BY-SA-4.0 design prose. Exact defaults and alternatives are in DECISIONS and each module.

## Review corrections

Use per-device AND per-audience/resource signing chains, so selective federation does not require private predecessors. Include worker-owned game state in coordinated backups. Preserve first valid intake provenance without letting quarantined bytes claim courier credit. Separate convergent replicated projections from local receipt/credit history. Require character-owner authorization bound to the destination. Make delayed private-client, suppression and recovery behavior explicit. Assign future integrations to consumers and repair missing dependencies.

## Consequences and evidence

This is a reviewed design baseline, not a code/hardware test result. All implementation modules remain Unstarted. G0-local allows independent local work after its required evidence; G2 must wait for private-client/crypto checks. M19/G4 delivers the core pilot; G5 finishes approved expansions and their added acceptance checks. Numeric capacity claims await measurements on named hardware.

The [review record](../development/REVIEW-2026-10-25.md) accounts for every module and correction. The [roadmap index](../development/README.md) and machine-readable graph agree on dependencies. Changing a selected decision requires a reasoned successor ADR and updates to affected contracts; ordinary implementation within these choices is already authorized.

## Licensing

The license choice is adopted, not a temporary revocable grant. This ADR’s experimental status concerns technical fitness. Repository-owned software/tools/examples use AGPL-3.0-or-later; design prose uses CC-BY-SA-4.0; third-party content retains its notices and user content is unaffected. See the root license notice and included texts.
