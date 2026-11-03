# M01 — Offline appliance and local access

Brief revision 1.1.19 · Roadmap baseline 1.1.0 · 2026-11-02 · Locked scope · Product baseline 0.6.0.

**State:** Complete as a software provider under continued development authority after Windows/Linux A–C evidence. Actual Debian install, isolated network/OS boot, renewal, diagnostics, capture and fresh-media checks pass. G1 physical router/radio/device acceptance remains pending. M12 consumes the lifecycle, compiled-profile, registry-bound maintenance and local release/control providers. Normal serving requires initialized identity; installed registration binds startup.

## Read only this packet

[START_HERE](../START_HERE.md), this brief, and [C01](../contracts/C01-storage-runtime.md).
Look up only decision rows T01, T07, T14 in [DECISIONS](../DECISIONS.md). Dependency briefs are not required reading if their contract/evidence is current.

**Source scope:** R02, R04, R05, P04, P09, AC01, AC02. **Target gate:** G1. Scope summaries below preserve these source IDs; reconcile conflicts with the current product requirements and user instructions.

**Prerequisites:** [M00](M00-foundation-spikes.md) G0-local, [M02](M02-storage-jobs.md), [M03](M03-identity-access.md), [M04](M04-signed-events.md) completed providers.

## Product outcome and boundaries

Run on a repurposed computer and isolated Wi-Fi. Infrastructure must never seek the internet; visitor cellular connectivity is allowed.

## Locked implementation

Use a Debian stable base, systemd service, Rust binary/static assets, local DNS/address card, and nftables rules that restrict routes to configured local client/peer networks and prevent WAN forwarding in IPv4 and IPv6. Use the M00-tested local HTTPS/trust path. No online DNS, NTP, ACME, telemetry or package manager activity in the deployed profile. Build releases on a separate developer machine and transfer artifacts locally.

## Ownership and interfaces

deploy/ and crates/node-runtime/ own installation config, local address/certificate state, service manifests and local-only diagnostics. Runtime registers real M02–M04 providers and owns no member keys. See the [runtime reference](../../../crates/node-runtime/README.md).

M12 consumes the same installed binary/registry for local backup/key generation/isolated restore. Its helper handles a service maintenance window and root-owned configuration secrets; restoration does not activate service/signing. M01 provider completion is unchanged.

Runtime/status code, initialization/payload tools and a verified developer first-install/service fixture exist. B adds explicit dual-stack listeners/local DNS-IP origins and prepared interface/subnet firewall/DNS files. Inactive-tree host/network/LAN certificates and actual boot fixtures pass CI; device enrollment remains a G1 check. Consume the contracts in the reading packet; changes need affected-consumer updates. Disabled optional modules retain data and preserve unrelated services. See the [install fixture](../../../deploy/README.md).

## Sequential work slices

1. A: offline install and service startup with local status page. Runtime/initialization/payload and actual Debian developer installation/service/OS reboot passed CI; physical reference-machine evidence remains open.
2. B: router/address enrollment and verified packet-boundary configuration. Explicit aliases/multiple listeners pass locally. Generated network/DNS boundaries pass Linux CI, including zero WAN IP frames. Public connection-card/CA export passes local checks. Full host boot/LAN certificate setup passes CI; device enrollment remains at G1.
3. C: restarts, local diagnosis and prepared-media reinstall procedure. Read-only diagnostics, local leaf renewal and clock/fresh-media reinstall instructions exist. Real renewal/restart, diagnostics and both complete boot captures pass. Two fresh offline installs pass with distinct node IDs and unchanged retained source; physical reference execution remains at G1.

## Completion evidence

- AC01 packet capture includes bootstrap/restart and both IP families.
- Local UI remains reachable with no peers and no WAN.
- The documented software installation path passes on the Debian 13 CI target. Physical reference-machine/router/device execution remains G1 acceptance.

## Locked defaults and verification

Debian 13 (trixie) amd64 is the selected first full-node/courier target; systemd, nftables, local DNS and a per-node local CA are selected. Enrollment compares CA fingerprints out of band; use node-ID-suffixed home.arpa names and local IP fallback. Never install a peer CA automatically. Document manual/local certificate renewal and clock setup. Browser trust enrollment must pass M00; no public ACME/NTP/DoH fallback. Test router default routes and both IP families.

## Handoff

Record completed slices, approved ADRs, contract/schema revisions, evidence and commit, remaining limitations and next slice in [WORK_LEDGER](../WORK_LEDGER.md). Existing scopes are approved. Every design, goal or feature change updates affected roadmap canon before completion; follow START_HERE. See [coverage](../COVERAGE.md) when changing source IDs.
