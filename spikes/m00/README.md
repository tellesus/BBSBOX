# M00 foundation experiment

Implementation/runbook reference for the [canonical M00 brief](../../docs/development/modules/M00-foundation-spikes.md). Current product decisions live in the roadmap and user instructions. This experiment implements the first local page and signed-message/storage round trip; the full M00 gate remains open for real-device and private-client evidence.

## What you can try

A static SvelteKit page served by a Rust/Axum HTTPS node. Leave a public test message, refresh, restart the node and verify it is still present. Each message uses a deterministic CBOR body, a COSE_Sign1 Ed25519 commitment and a SHA-256 event ID; the node verifies its chain before returning it. A name is a display label, not an authenticated person. The signing key is a persistent node fixture held on this computer. No account/private-group implementation is implied.

The server binds to `127.0.0.1:8443` by default and makes no outbound network requests. Build tools fetch dependencies during development. Assets are served locally at runtime. The experiment holds at most 1,000 messages and returns the newest 100; no automatic eviction. Its `bbsbox.m00.event/v0` format is disposable and has no compatibility promise with protocol v1.

## Build and run

Prerequisites: Rust 1.99.1 with a native C compiler, Node 24.22.1 (minimum 22.12), npm, Python 3.11+ and OpenSSL 3.x. Production Debian support has not been tested. On a Debian development computer, install its normal build tools/OpenSSL and the pinned Rust/Node toolchains. No Windows/phone compatibility claim follows from this recipe alone.

From this directory:

```sh
cd web
npm ci
npm run check
npm run build
cd ..
python tools/init_tls.py
cargo build --locked --manifest-path node/Cargo.toml
cargo run --locked --manifest-path node/Cargo.toml
```

Open **https://127.0.0.1:8443**. The browser must trust this test CA through a deliberate local enrollment. The TLS helper prints the CA SHA-256 fingerprint; compare it out of band before trusting `.state/tls/ca.pem` in a dedicated test browser profile. The helper never modifies system trust. Keep the CA private key and server key on the node. Do not click through arbitrary certificate errors. Leaf certificates expire after 30 days; generate a fresh test state/enrollment when needed.

This Windows workspace has portable Rust/LLVM tools under ignored `.tooling/`. Before building/running in PowerShell, dot-source `./tools/local-env.ps1`; it changes only that shell. The portable Windows toolchain requires its LLVM DLL directory on PATH, including when launching the binary. The installed Node at `C:\Program Files\nodejs` is too old for this build; use Node 24.22.1. The tested compiler is Rust 1.99.1 `x86_64-pc-windows-gnullvm`; normal Windows MSVC builds require their own evidence.

All state is in `.state/` and ignored by Git. Preserve its database/WAL and fixture key together. Never replace a missing fixture key over an existing database. This spike's cold-restart test is not a production backup/restore or signing-rollback solution; M12 owns that contract. Stop with Ctrl+C.

## Verification

```sh
cargo test --locked --offline --manifest-path node/Cargo.toml
cargo clippy --locked --offline --manifest-path node/Cargo.toml --all-targets -- -D warnings
cd web
npm run check
npm run build
npx playwright install chromium
npm test
```

Use the first dependency-fetch/build without `--offline`; repeat Rust verification with the cached, locked dependencies afterward. Browser installation is a development download. With a locally installed Edge, set `BBSBOX_BROWSER_CHANNEL=msedge` instead. The browser tests start and stop their own server on 9443 using fresh ignored state; do not run another server there. They trust only the generated leaf's SPKI in that temporary browser process, not every certificate, and separately verify CA-based TLS acceptance/rejection through Node. They do not install a CA into system trust.

Rust checks cover signature/body/chain tampering, wrong signing key/incarnation, SQLite reopen/retry, corrupted data, HTTP/1 host and HTTP/2 authority handling, foreign origins and input validation. Browser checks exercise actual HTTPS posting/reload, HTML shown as literal text, failed-send draft retention, retry deduplication, process restart, narrow layout and absence of application requests outside this node during the checked journey. Node crypto plus an independent CBOR decoder also verifies an exported signature/body commitment. Narrow desktop browser layout is not an Android/iOS test.

## Next physical-device check

On an isolated test LAN, generate a fresh TLS state with `python tools/init_tls.py --state .state/lan --san YOUR_LAN_IP`. Set `BBSBOX_STATE=.state/lan` and `BBSBOX_BIND=YOUR_LAN_IP:8443`, then start the node. Allow the specific local port through the host firewall only for that test LAN if required. Visit the IP from the chosen phone after explicit fingerprint/trust enrollment. Verify all page requests, HTTPS/secure-context support, post/reload/restart and no infrastructure internet attempts with WAN absent. The helper supports additional local DNS names using repeated `--san` arguments; it does not create DNS records or open a firewall.

The [OpenMLS experiment](mls/README.md) now exercises disconnected writers, removal, retention, competing commits and encrypted client snapshots. The test recovery guard keeps restored clients receive-only until a fresh owner-authorized join. M00 still needs Debian reference-hardware measurements, actual Android Chrome/iOS Safari enrollment, independently signed Tauri-client trust and production recovery integration. Phone tests are scheduled at the integrated local-node milestone and before private-client release. Record actual gate results in the canonical work ledger.

Resolved dependency names/versions/license metadata, including development and optional packages, are in [dependencies.json](dependencies.json). The upstream packages retain their own notices. Refresh the inventory with `tools/dependency_inventory.py` and filtered Cargo metadata when dependencies change; absent foreign-platform license metadata is marked for upstream inspection rather than inferred. Production source/license packaging is owned by M12.

## Linux network-isolation check

The Windows-local Python tests validate the trace auditor. The actual isolation experiment requires a disposable Linux developer environment with `unshare`, `iproute2` and `strace`. The GitHub workflow runs it after building the same node and generating local test certificates:

```sh
python3 -m unittest discover -s tools -p test_offline_smoke.py -v
sudo unshare --net -- bash -c 'ip link set lo up && python3 tools/offline_smoke.py'
```

The child namespace has only loopback and no IPv4/IPv6 gateway/default route. The script tests real HTTPS/static assets, signed storage and retry on IPv4 and IPv6. It audits the actual node's network syscalls; attempted outbound connections fail the check even if routing would have blocked them. Empty traces cannot pass. Reports/traces are saved in ignored `.state/` and uploaded by CI. This establishes a bounded process-level isolation result; it does not provision or validate a physical Debian appliance, router, LAN DNS or phone.
