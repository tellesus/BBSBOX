# M01 A — local node runtime

Implementation reference for [M01](../../docs/development/modules/M01-offline-appliance.md) and [C01](../../docs/development/contracts/C01-storage-runtime.md). Product canon remains the roadmap and current user instructions. This is A's runtime checkpoint; Debian installation, network provisioning and M01 B/C remain pending.

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

For Linux isolation evidence, build first on the developer machine, install developer test tools there, then run in a disposable namespace:

```sh
sudo unshare --net -- sh -c 'ip link set lo up && python3 tools/runtime_smoke.py --isolated'
```

The existing independently tested M00 syscall auditor requires actual listener/accept traffic and rejects outbound connect/explicit datagram destinations. Both families include initial boot and restart traces. CI stores the report/traces as artifacts. This proves process behavior in that environment, not the full appliance's system services, router/DNS setup or Debian installation path.

## Configuration and deployment interface

[The example](../../deploy/node.example.json) uses explicit loopback port 8443. Config format `bbsbox.node-config/v1` is bounded to 64 KiB, rejects unknown fields, and requires distinct absolute database/certificate/key paths. Listener addresses must be literal loopback/private/link-local addresses on an unprivileged port; wildcard/public binds fail. The origin must be bare HTTPS with the same port. Local DNS/address aliases and certificate enrollment are M01 B work; the server performs no DNS lookup, time synchronization, ACME request, update fetch or telemetry.

TLS material must already exist and parse before storage/listener startup. The primary configured authority must match each request; a conflicting Host header is rejected. The bundled status page uses self-hosted CSS/JavaScript, text-only DOM assignment, dark colors on black and a restrictive content policy. It contains functional status copy. No CDN or remote embed is required.

[The systemd unit](../../deploy/systemd/bbsbox.service) defines the intended `/opt/bbsbox/current`, `/etc/bbsbox` and `/var/lib/bbsbox` layout, unprivileged user, private state and a 20-second service stop bound around the runtime's 10-second connection drain. Install/user creation/TLS ownership and actual unit execution remain the next A slice; this checked-in unit alone is not an installed or verified appliance. M12 supplies signed release/update and coordinated backup activation. Its full integration depends on M01 completion.

The selected first appliance target is [Debian 13 trixie amd64](https://www.debian.org/releases/trixie/), the verified stable family as of 2026-10-31. Service directives follow the official [systemd service](https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html) and [execution environment](https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html) documentation; the actual target version still needs execution evidence. Neither this reference nor a test CA is a release trust root.

Three Rust tests cover bounded config validation, literal/local addressing, exact status surface, real registered migrations, restart identity and exclusive writer ownership. The real-process HTTPS smoke complements those checks. M01 remains In progress until offline installation/service execution, full-host network boundaries, local address enrollment and recovery/reinstall procedures pass.
