# M01 — Offline appliance and local access

Version 0.1.0 · 2026-10-25 · Tier 2 proposal · Tier 1 baseline 0.4.0.

**State:** Planned capability; implementation choices proposed. No implementation or passed checks claimed.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C01](../contracts/C01-storage-runtime.md).
Look up only decision rows T01, T07, T14 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R02, R04, R05, P04, P09, AC01, AC02. **Target gate:** G1. Scope summaries below preserve these source IDs; refer to the specific Tier 1 section if a conflict appears.

**Prerequisites:** [M00](../modules/M00-foundation-spikes.md).

## Product outcome and boundaries

Run on a repurposed computer and isolated Wi-Fi. Infrastructure must never seek the internet; visitor cellular connectivity is allowed.

## Recommended implementation

Propose a Debian stable base, systemd service, Rust binary/static assets, local DNS/address card, and nftables rules that restrict routes to configured local client/peer networks and prevent WAN forwarding in IPv4 and IPv6. Use the M00-tested local HTTPS/trust path. No online DNS, NTP, ACME, telemetry or package manager activity in the deployed profile. Build releases on a separate developer machine and transfer artifacts locally.

## Ownership and interfaces

Proposed deploy/ and crates/node-runtime/; installation config, local address/certificate state, service manifests, local-only diagnostics. Does not own user keys.

Paths above are proposed implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: offline install and service startup with local status page.
2. B: router/address enrollment and verified packet-boundary configuration.
3. C: restarts, local diagnosis and prepared-media reinstall procedure.

## Completion evidence

- AC01 packet capture includes bootstrap/restart and both IP families.
- Local UI remains reachable with no peers and no WAN.
- A host follows the documented installation path on the reference machine.

## Decisions before completion

Record the first OS/CPU/router profile, local address convention and certificate renewal workflow; do not advertise arbitrary-router support.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). A passing fixture is evidence for this module, not approval of new product scope. See [coverage](../COVERAGE.md) when changing source IDs.
