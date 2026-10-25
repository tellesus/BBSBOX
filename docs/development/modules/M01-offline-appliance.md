# M01 — Offline appliance and local access

Version 1.0.1 · 2026-10-25 · Locked development baseline · Product baseline 0.5.1.

**State:** Approved for implementation under U19; unstarted. Runtime and hardware evidence remains pending.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C01](../contracts/C01-storage-runtime.md).
Look up only decision rows T01, T07, T14 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R02, R04, R05, P04, P09, AC01, AC02. **Target gate:** G1. Scope summaries below preserve these source IDs; reconcile conflicts with the current product requirements and user instructions.

**Prerequisites:** [M00](M00-foundation-spikes.md).

## Product outcome and boundaries

Run on a repurposed computer and isolated Wi-Fi. Infrastructure must never seek the internet; visitor cellular connectivity is allowed.

## Locked implementation

Use a Debian stable base, systemd service, Rust binary/static assets, local DNS/address card, and nftables rules that restrict routes to configured local client/peer networks and prevent WAN forwarding in IPv4 and IPv6. Use the M00-tested local HTTPS/trust path. No online DNS, NTP, ACME, telemetry or package manager activity in the deployed profile. Build releases on a separate developer machine and transfer artifacts locally.

## Ownership and interfaces

Planned deploy/ and crates/node-runtime/; installation config, local address/certificate state, service manifests, local-only diagnostics. Does not own user keys.

Paths above are planned implementation locations, not existing code. Consume the contracts in the reading packet; changes to them need affected-consumer updates. A disabled optional module retains its data and does not break unrelated local services.

## Sequential work slices

1. A: offline install and service startup with local status page.
2. B: router/address enrollment and verified packet-boundary configuration.
3. C: restarts, local diagnosis and prepared-media reinstall procedure.

## Completion evidence

- AC01 packet capture includes bootstrap/restart and both IP families.
- Local UI remains reachable with no peers and no WAN.
- A host follows the documented installation path on the reference machine.

## Locked defaults and verification

Debian stable x86-64 is the first full-node/courier target; systemd, nftables, local DNS and a per-node local CA are selected. Enrollment records the CA fingerprint out of band; use locally resolved names containing a node-ID suffix and local IP fallback. Never install a peer CA automatically. Manual/local certificate renewal and local clock setup are documented. Browser trust enrollment must pass M00; no public ACME/NTP/DoH fallback. Router configuration is tested for default routes and IPv6 as well as IPv4.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
