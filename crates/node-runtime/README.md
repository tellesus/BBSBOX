# Local node runtime and maintenance commands

Implementation reference for [M01](../../docs/development/modules/M01-offline-appliance.md) and [C01](../../docs/development/contracts/C01-storage-runtime.md). Product canon remains the roadmap and current user instructions. M01 is complete as a software provider; [deployment evidence](../../deploy/README.md) covers actual Debian installation, network provisioning and B/C lifecycle checks. Physical router/radio/phone acceptance remains G1.

`bbsbox-node --config <node.json>` starts the real M02 single writer with M03 identity and M04 event migrations, activates the node signer, and serves bundled status assets plus `/api/v1/node` over TLS. Node identity survives restart; restored recovery-marked storage cannot activate. The runtime retains the signing capability privately. Its read-only public status exposes format/name/node ID/software version/readiness, with no private state or account provisioning endpoint. M05 owns the community shell/authentication routes.

[The OpenAPI contract](../../protocol/http/node-runtime.openapi.json) describes that versioned status response; its required fields are checked against the actual route response.

## Local verification

Use the installed project tools described in [M00](../../spikes/m00/README.md); Windows builds require its local LLVM runtime libraries on PATH. From the repository root:

```powershell
. ./spikes/m00/tools/local-env.ps1
cargo test --locked --offline -p bbsbox-node-runtime
cargo build --locked --offline -p bbsbox-node-runtime
python tools/runtime_smoke.py
```

The smoke tool creates disposable state and a development CA using M00's test-only TLS fixture; it never installs trust or modifies live node data. It opens actual IPv4/IPv6 HTTPS connections, verifies trust failure without the CA, serves both packaged assets, checks authority/method rejection and restarts the process against the same database. Windows termination is a process-loss check; the lifecycle test separately exercises a drained writer shutdown. Linux also exercises SIGTERM. Phone enrollment waits for G1/G2.

`bbsbox-node --config <node.json> --initialize` prepares the database/node identity and exits with `bbsbox.node-init/v1` public metadata. It needs no TLS files and opens no listener. Provisioning can obtain the node ID before making its local certificate/address card. Later server starts retain that identity. The smoke check now initializes before generating TLS, and its Linux audit checks that phase separately for successful exit without a listener/outbound attempts.

For Linux isolation evidence, build first on the developer machine, install developer test tools there, then run in a disposable namespace:

```sh
sudo unshare --net -- sh -c 'ip link set lo up && python3 tools/runtime_smoke.py --isolated'
```

The existing independently tested M00 syscall auditor requires actual listener/accept traffic and rejects outbound connect/explicit datagram destinations. Both families include initial boot and restart traces. CI stores the report/traces as artifacts. This proves process behavior in that environment, not the full appliance's system services, router/DNS setup or Debian installation path.

## Configuration and deployment interface

[The example](../../deploy/node.example.json) uses explicit loopback port 8443. Config format `bbsbox.node-config/v1` is bounded to 64 KiB, rejects unknown fields, and requires distinct absolute database/certificate/key paths. Listener addresses must be literal loopback/private/link-local addresses on an unprivileged port; wildcard/public binds fail. Optional `additional_binds` admits up to eight unique additional addresses on that same port, supporting simultaneous IPv4/IPv6. Every listener binds successfully before HTTP activation; stop/error drains all listeners and the shared writer.

The primary `origin` and up to eight optional `additional_origins` must be unique bare HTTPS origins on the listening port. Existing configuration without either additional field remains valid. Requests must match one exact configured authority; conflicting Host/URI addresses are rejected even when both are configured aliases. TLS material must cover each advertised address and parse before storage/listener startup. The bundled status page uses self-hosted CSS/JavaScript, text-only DOM assignment, dark colors on black and a restrictive content policy. The server performs no DNS lookup, time synchronization, ACME request, update fetch or telemetry. M05 must enforce origin/session policy on its eventual account routes; address aliases alone grant no account access.

[The systemd unit](../../deploy/systemd/bbsbox.service) uses the `/opt/bbsbox/current`, `/etc/bbsbox` and `/var/lib/bbsbox` layout, unprivileged user, private state and a 20-second service stop bound around the runtime's 10-second connection drain. Install/user creation/TLS ownership and actual unit execution/restarts/reboots pass the Debian fixture. M12 owns signed release/update and coordinated recovery activation.

[The developer install fixture](../../deploy/README.md) implements first install, actual service/network boot, local renewal/diagnostics and fresh-media reinstall. Its software checks pass CI; physical appliance acceptance remains pending.

The selected first appliance target is [Debian 13 trixie amd64](https://www.debian.org/releases/trixie/), verified as of 2026-10-31 and exercised by the Debian fixture. Service directives follow the official [systemd service](https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html) and [execution environment](https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html) documentation. Neither this reference nor a test CA is a release trust root.

Four base Rust tests cover bounded config validation, literal/local addressing, exact status surface, real registered migrations, offline initialization, restart identity and exclusive writer ownership. The real-process HTTPS smoke and Debian lifecycle checks complement them. M01 software completion does not grant G1 physical device acceptance or M12 recovery activation.

## Prepared development payload

On a Linux developer/build machine, after building the executable:

```sh
python3 tools/runtime_bundle.py build --binary target/debug/bbsbox-node --output work/runtime-bundle
python3 tools/runtime_bundle.py verify work/runtime-bundle
```

[The builder/verifier](../../tools/runtime_bundle.py) copies the ELF64 amd64 executable, candidate unit, config example and license notices into a fresh destination. A bounded canonical manifest lists exact bytes/SHA-256 values and the source commit/dirty flag; it is written last and existing destinations are refused. Verification rejects incomplete/changed/unlisted files, path escapes, duplicates, symlinked members, unknown formats and wrong executable targets. It performs no download, install or service activation.

This format is explicitly a development payload, supplying integrity/target checks for local install tests. M12 owns authenticated signed releases, installed trust roots, dependency/source packaging and updates. No production release or completed appliance installation is implied by preparing this payload. CI checks a real Linux binary; portable unit fixtures use a labeled synthetic ELF header and never execute it.

## Local maintenance commands

The same installed binary exposes local `backup-keygen`, `backup` and `restore` commands. Each rejects unknown/duplicate/missing options and opens no HTTP listener. These consume [M12 snapshot/archive primitives](../maintenance/README.md), not a separate migration registry.

```sh
bbsbox-node backup-keygen --output /private/operator-backup.key
bbsbox-node backup --config /etc/bbsbox/node.json --node node:EXPECTED_DIGEST --recipient age1RECIPIENT --output /media/backup/node.age --staging /private/local-staging
bbsbox-node restore --archive /media/backup/node.age --node node:EXPECTED_DIGEST --identity-file /private/operator-backup.key --output /private/isolated-restore --staging /private/local-staging
```

Replace example identities/paths with real values. Key generation writes a fresh private file and prints only its public recipient. Keep the operator identity separately from removable backup media. Backup requires a stopped/drained source service and serializes against TLS renewal. Read-only preflight rejects missing/mismatched identity or registry before writer startup, preventing accidental initialization or schema upgrades. Current scope is the foundation node with no installed games. The [Debian helper](../../deploy/backup_development.py) handles service stop/resume, root-owned CA staging and known source-file ownership; prefer it on the installed development node.

Restore writes a fresh isolated copy; configuration remains staged and the database keeps its persistent recovery block. It never installs files, removes recovery markers, resumes signing, or activates a second copy of the source. Local tests exercise real providers, wrong/live/missing sources, configuration lock exclusion, output preservation and private temporary-stage cleanup. Actual Debian backup/restore, CA continuity, source ownership/resumption, inactive/failing backup behavior and later reboot/network checks pass [CI](https://github.com/tellesus/BBSBOX/actions/runs/41375010768). Recovery reconciliation/activation and production release trust remain M12 work.

## Compiled release profile

`bbsbox-node --release-profile` prints bounded `bbsbox.release-runtime/v1` metadata without reading configuration, opening a database/signer or binding a listener. It reports the crate version, compiled target, current core schema, directly readable core schemas and sorted exact compiled module migration SQL hashes. The current direct-reader claim is schema 3 only; older storage needs migration, and this report does not grant rollback compatibility. Non-Linux/amd64/GNU builds report their actual unsupported platform instead of claiming Debian compatibility.

M12 compares this independently obtained report against a signed release manifest after authenticating/staging the candidate. The report itself is not a signature, data compatibility proof or installation permission. A real-binary local test checks exact metadata and absence of created state. Linux additionally signs/stages the actual executable, executes its profile command and rejects an authenticated false migration claim. That fixture uses labeled material placeholders and is not an installable source-complete release.

## Release producer/public inspection commands

The binary exposes `release-keygen --output`, `release-sign --bundle --manifest --key-file`, `release-inspect --bundle --public-key` and `release-stage --bundle --public-key --output`. All options require exact values; duplicates/unknown options fail. These commands open no node configuration, database or listener and never change installed authority/selection or remove recovery blocking.

Key generation is an explicit producer action: a fresh private file contains `BBSBOX-RELEASE-KEY-V1`, newline, 64 lowercase hexadecimal seed characters and a final newline. The in-memory seed/file buffer zeroize; output contains only format, public key and release-role ID. The private file remains outside the payload, and existing output is refused. Backup/person/node key formats cannot be imported as release secrets.

Signing takes bounded manifest input and an immutable producer-controlled unsigned payload. It checks exact files/hashes and the ELF target before publishing `manifest.cose`, then re-verifies the completed payload. This producer command authenticates declared contents; full material semantics and the actual compiled report are checked by [the candidate tool](../../tools/release_candidate.py). Public inspection/staging requires an independently selected 32-byte public key encoded as 64 lowercase hexadecimal characters. Reports identify the authenticated release and expected compiled profile, with `installation_authority:false`.

Two portable operator tests check producer-key separation/output preservation, signing/inspection/private-copy round trips, tampering, role/inventory mismatch and strict options. Unix adds private-mode/link checks. The containing-commit Linux fixture combines a real rebuilt binary, complete materials, an ephemeral test authority, private staging and bounded profile execution. No production release key is generated by CI or these documentation examples.
