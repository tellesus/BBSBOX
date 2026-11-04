# M01 developer installation fixture

Reference for [M01](../docs/development/modules/M01-offline-appliance.md); the canonical roadmap and user instructions own product scope. This fixture exercises first installation on an isolated Debian 13 amd64 developer target. M12 owns authenticated production releases/updates. The install/network/renewal/reboot software checks pass [Linux CI](https://github.com/tellesus/BBSBOX/actions/runs/41342181589). Physical router/radio/device acceptance remains at G1; the fresh-media fixture also passes [CI](https://github.com/tellesus/BBSBOX/actions/runs/41342863072).

## Prepared inputs and first install

Prepare Python 3, OpenSSL, the standard account utilities, systemd and the binary's local runtime dependencies on the developer target. The installer checks those tools and never invokes a package manager or downloads material. Build and verify the [development payload](../tools/runtime_bundle.py) on the developer/build machine, then transfer it and these project scripts locally.

On the isolated test target, from a checkout of the matching source:

```sh
sudo python3 deploy/install_development.py --bundle /path/to/runtime-bundle --development --enable
```

`--enable` installs the unit's next-boot link; it does not start a service. This first-install fixture refuses existing node configuration, TLS material, activated binaries/units or nonempty state. It does not replace a live installation or initialize over community data. First installation binds loopback. The next-boot preparation below installs the local network profile.

The installer verifies the exact payload inventory, creates an unprivileged `bbsbox` OS account, installs immutable runtime files under `/opt/bbsbox/development/<manifest-sha256>`, and writes the [unit](systemd/bbsbox.service), `/etc/bbsbox/node.json` and private `/var/lib/bbsbox` state. Offline initialization obtains the stable node ID before certificate generation.

Each node gets its own local CA and a leaf for localhost, both loopback IPs and a node-ID-suffixed name under [home.arpa](https://www.rfc-editor.org/rfc/rfc8375.html). The CA private key is root-only; the service can read its leaf key and configuration but cannot replace them. The resulting public CA fingerprint/installation metadata is recorded locally. No trust store or peer trust is automatically installed. The network profile supplies local DNS and the public card below supplies visitor connection details; phone trust enrollment is verified at G1.

Before enabling the unit, installation checks those read permissions as the actual service account, including denial of CA-private-key access. Directory modes are set explicitly after the private provisioning umask.

The installer also writes a public connection card under `/opt/bbsbox/enrollment`: `connect.html`, `node.json` and `ca.pem`. Copy those three files to local visitor media or print the card. The card lists configured local addresses, public node ID and the CA certificate's SHA-256 fingerprint. Compare that fingerprint through a separate SysOp-supplied copy before enrolling trust. The export includes no private key or database/configuration path and changes no trust store. Device-specific enrollment still needs the planned phone milestone tests.

To prepare a fresh card after a configuration change, run `sudo python3 deploy/enrollment.py --output /path/to/new-card-directory` from the matching source checkout. The tool checks that the CA matches the installation record and that addresses are local; an existing output directory is preserved and rejected. CA fingerprint checking here uses local certificate/time checks without online lookups.

## Actual Debian service verification

[The CI image recipe](ci/Dockerfile) prepares dependencies from the official Debian trixie image on a developer/build network. Its installation instruction then runs with networking disabled. The image is a developer test tool; production does not require Docker or a registry.

[The context/export helper](ci/prepare.py) copies only the selected payload/scripts into a fresh workspace context. Before extracting the exported OS, it validates archive paths, symlink ancestry, hardlink targets and entry types. The target must be a fresh directory beneath repository `work/`.

[The boot check](ci/boot_service.py) starts the prepared tree with actual systemd under [systemd-nspawn](https://www.freedesktop.org/software/systemd/man/latest/systemd-nspawn.html), in a separate PID/network namespace containing only loopback and no gateway. It identifies that exact OS root before entering it, checks the real unit/cgroup/unprivileged process and verified HTTPS, restarts the unit, switches the fixture to IPv6 and boots the OS again with the same node identity. Shutdown signals only the child manager it started. [nsenter](https://man7.org/linux/man-pages/man1/nsenter.1.html) supplies access to that validated test namespace.

Actual Debian offline installation, unit/restart/reboot and network lifecycle checks pass CI. Windows runs the portable preservation, certificate, parser and runtime checks. Physical reference-machine, router/radio and phone tests remain release-gate evidence. M12 owns signed releases, coordinated backup and recovery activation.

## Local network profile preparation

[The profile preparer](network_profile.py) takes an explicit [local-interface configuration](network.example.json) and writes a fresh output directory without activating anything:

```sh
python3 deploy/network_profile.py --profile deploy/network.example.json --output work/local-network
```

Replace the example hostname with the installation's node-ID-suffixed name and select actual interface names/static addresses. Networks must be nonoverlapping RFC1918 IPv4 or ULA IPv6 subnets. This profile uses the isolated router for client address assignment; it does not run DHCP or provide internet routing. Peer interfaces require an explicit `peer` role; the shipped example has none. Application federation still requires separately approved peers.

Set `"prepare_game_host": true` in the network profile to reserve the corresponding `bbsbox-game-<node suffix>.home.arpa` name on local DNS and include it in the node's TLS leaf. The alias uses the same local addresses and HTTPS port. The public connection card lists the core addresses. Game activation is a separate appliance step.

Prepared files include static networkd settings without DHCP/default gateway/router advertisements, forwarding-off sysctls, a DNS-only dnsmasq configuration, exact DNS/IP runtime origins/listeners, and an inet nftables boundary. Rules bind each allowed subnet to its named interface and node address. Clients can reach HTTPS/local DNS; new node-originated HTTPS is restricted to explicitly configured peer interfaces. Established replies are permitted only inside those same interface/address boundaries, so an already established WAN connection receives no exemption. Other input/output and all forwarding drop in both IP families. IPv6 neighbor discovery is limited to configured local links.

The DNS process must use only the generated `--conf-file`, without additional configuration directories or upstream arguments. It serves the exact local hostname using [dnsmasq's](https://thekelleys.org.uk/dnsmasq/docs/dnsmasq-man.html) local-only mode, ignores resolver/hosts files and has no upstream server. The [nftables reference](https://netfilter.org/projects/nftables/manpage.html) defines the inet hooks, interface matches and state handling used here.

[The isolated boundary test](ci/network_boundary.py) creates simulated client, peer and WAN interfaces/default routes, checks both IP families, local DNS/HTTPS-port traffic, peer initiation, blocked client initiation/forwarding and blocked new/already-established WAN traffic. It captures outgoing IP frames on the simulated WAN and saves counters. These rules/DNS assertions pass Linux CI, including zero outgoing IP frames on the simulated WAN. Host preparation and full boot verification are covered below; hardware/device acceptance stays at G1.

## Inactive Debian tree network preparation

[The next-boot preparer](configure_network.py) configures an inactive developer OS tree after the first install, using the same node identity and local CA. It refuses the running OS root, a tree currently used by a process, existing local network files and interrupted preparation. It does not start any service or modify community storage. Set the input profile's `hostname` to `auto` to derive the installed node name, or use that exact name explicitly:

```sh
sudo python3 deploy/configure_network.py --root work/debian-root --profile work/network-install.json --development
```

Prepare Debian's nftables, dnsmasq-base, iproute2 and systemd dependencies before isolating the image. This targets the dedicated minimal Debian tree, not migration of an existing workstation/router. The preparer writes static networkd files, a firewall-before-network dependency, dedicated DNS service, local resolver settings and masks for automatic online package/time/update/discovery services. LAN leaf certificates are signed by the existing CA; the current public card is regenerated and the loopback card is retained as `enrollment-loopback`. Private CA/node/member keys are not replaced. An incomplete marker blocks runtime startup after an interrupted preparation; inspect and finish/rebuild that candidate before boot.

[The network boot fixture](ci/boot_network.py) uses actual Debian systemd/networkd/nftables/dnsmasq and the real HTTPS runtime over a simulated visitor link. It checks DNS, both address families, node/CA continuity, disabled forwarding/online services and a second OS boot without peers. Those software assertions pass CI; physical router/radio and visitor enrollment remain at G1. The generated [networkd settings](https://www.freedesktop.org/software/systemd/man/latest/systemd.network.html) use static addresses without DHCP, router advertisements or a default route.

The DNS unit uses normal foreground mode and drops to its dedicated `bbsbox-dns` account after binding. Its real process must have that non-root user ID in all UID slots. The vendor DNS/firewall units and kernel-command-line network generator are masked so they cannot compete with the selected profile. Only `bbsbox-boundary.service` loads the owned firewall table.

The boot fixture now also records bounded PCAP files from before the simulated link is attached through boot, local certificate renewal/runtime restart and OS shutdown. [The frame auditor](ci/link_capture.py) distinguishes IPv4/IPv6 visitor-subnet traffic and local IPv6 link-control destinations from nonlocal destinations; malformed/unsupported nested frames fail the audit. Each boot must observe both IP families and no nonlocal destination or capture error. Parser tests and both complete boot captures pass CI: 47 IPv4 and 22 IPv6 guest frames in the recorded run, with no nonlocal destination or capture error. These simulated-link captures complement the separate fake-WAN test; physical network/device acceptance remains at the release gates.

## Local diagnosis

Run `sudo python3 deploy/diagnose.py` from the matching source checkout on the Debian node. It reads service state, local routes, firewall chain policies, online-service masks, certificate chain/expiry and the current UTC clock. It reports attention when a required check fails and returns a failing exit status. It never requests a DNS lookup or contacts a server; its JSON includes selected public node details and local diagnostic results. It does not read private key contents. CI runs this tool inside each prepared network guest boot; all recorded checks pass.

The certificate check warns when the leaf has less than a week left. This report alone does not establish full firewall, browser or device acceptance.

## Local clock and leaf renewal

Compare `date --utc` with a trusted local clock before issuing certificates or investigating date-related verification failures. Set the node's clock locally with `sudo date --utc --set='YYYY-MM-DD HH:MM:SS'`, replacing the placeholder with the correct UTC time. Keep the machine's hardware clock set correctly for later boots. The deployed profile masks automatic time synchronization and performs no online clock request.

[Local leaf renewal](renew_tls.py) uses the existing per-node CA and the currently advertised local names/addresses:

```sh
sudo python3 deploy/renew_tls.py --development
sudo systemctl restart bbsbox.service
sudo python3 deploy/diagnose.py
```

The tool generates a fresh one-year server key/certificate, verifies every local name, preserves the CA and node identity, and writes a complete protected pair in a new directory. It synchronizes the files/directories before atomically selecting both paths in the node configuration. The running process keeps its previous TLS context until restart. Existing certificate directories remain available for inspection. A local provisioning lock prevents concurrent renewals, and a changed configuration is refused before selection. No trust-store installation or network request occurs. Renewal under the same CA keeps the public enrollment fingerprint unchanged. An expired CA requires a separately planned CA replacement and visitor re-enrollment.

Two real-certificate tests pass locally: repeat issuance changes the server key/serial, preserves CA bytes and serial-file state, verifies intended names and rejects a wrong IP/public or injected names. CI additionally renews inside the actual guest, checks the running process still serves the prior leaf, restarts it and checks the new certificate fingerprint plus node/CA continuity. These actual guest renewal/restart and second-boot checks pass CI.

## Prepared-media reinstall check

Use fresh developer OS media with the prepared dependencies, matching source and verified runtime payload. Retain the original node's storage/configuration/TLS media intact. Follow first installation on the fresh media and verify its local status page, node ID, CA fingerprint and service restart. It creates an independent test node; record that its node ID differs from the retained source. Running first installation against existing state must fail and leave that state unchanged; the portable preservation tests cover this refusal. The coordinated original-node backup/restore/activation path is M12's next module. Reference-hardware execution remains an acceptance item.


The [fresh-media fixture](ci/reinstall.py) installs twice in separate fresh prepared OS layers with networking disabled and registry pulls forbidden. It requires two new node IDs distinct from the retained source and checks the source database/configuration/public CA bytes remain unchanged. The prepared Docker stage contains dependencies/source/payload but no initialized node; each run generates its own identity. The fresh-media fixture passes CI, with the retained source unchanged.

## Development backup and isolated restore

The [cold-backup helper](backup_development.py) uses the same installed runtime and migration registry. Generate the operator backup identity in a separate private location with `bbsbox-node backup-keygen --output /private/operator-backup.key`; it prints only the public `age1...` recipient. Keep the secret separately from backup media. On the Debian development node:

```sh
sudo python3 deploy/backup_development.py --development --recipient age1RECIPIENT --output /media/backup/node.age
```

Replace the recipient/path with real values. The helper serializes local maintenance, stops/drains an active service, captures the current node's database/blobs/configuration/server keys into root-private local staging, encrypts/signs the export, repairs only known source SQLite/lock ownership, and resumes a previously active service. It refuses existing output and output inside live node storage/configuration. Backup serialization also excludes concurrent leaf renewal. The CA and backup keys remain separate. This helper uses the foundation profile for nodes without game state.

Use `bbsbox-node restore --archive /media/backup/node.age --node node:EXPECTED_DIGEST --identity-file /private/operator-backup.key --output /private/isolated-restore --staging /private/local-staging` to verify/decrypt into a fresh isolated directory. It preserves source identity and writes `data/`, staged `configuration/` and a recovery report. It neither replaces live configuration nor starts a writer. The ordinary restore keeps its persistent marker; the explicit M12 isolated-authorization command can release it after recording the source decision. Client-held root recovery remains separate. Local provider/command tests and actual Debian ownership/resume/CA/isolated-restore assertions pass [CI](https://github.com/tellesus/BBSBOX/actions/runs/41375010768), including failed-backup resumption and inactive-source preservation. See the scheduled backup and fresh-appliance procedures below. M11 integrates the real game checkpoint adapters.

For a development node running the demo, stop the node and its game worker. Run `bbsbox-node backup-game-node` with the node configuration, expected node ID, recipient, fresh output file and a private staging directory outside live data and configuration. Use the matching demo worker beside `bbsbox-node`. Use `bbsbox-node restore-game-node` with the same restore options shown above to verify and stage the core, configuration and game checkpoint. The restore reports `recovery-required` and writes `game-recovery.json`. Signed appliance activation and scheduled game-node backups remain pending.

## Router/AP settings for the G1 hardware check

Use an isolated local SSID bridged to the node's client interface. For the current example profile:

| Setting | Local reference value |
| --- | --- |
| Node static addresses | 192.168.50.1/24 and fd42:bb50::1/64 |
| Router/AP LAN address | An unused address such as 192.168.50.254/24 |
| IPv4 client DHCP pool | 192.168.50.100–199; exclude static addresses |
| Client DNS | The node's local DNS addresses; no upstream resolver |
| IPv6 clients | Local ULA on-link/SLAAC prefix and local DNS advertisement; no advertised internet default route |
| Infrastructure WAN services | WAN disabled/disconnected; online time/update/cloud-management and DNS forwarding disabled |

Clients need the local subnet route; the appliance supplies no internet gateway. The node itself uses its static profile without DHCP/router advertisements/default routes. Use the printed DNS or IP connection address and independently compare the CA fingerprint. Record the exact router firmware, SSID and device enrollment results at G1. These settings are the physical test plan; the current namespace fixtures use static visitor addresses and do not certify a router model.


## Signed first-install integration (M12, in progress)

[The signed installer](install_release.py) targets a fresh Debian 13 amd64 appliance with an explicitly supplied protected bootstrap verifier and release public authority. It authenticates/private-stages the complete candidate and checks committed materials/assets/compiled profile before creating node identity. It refuses existing application/control or node state. The [signed unit](systemd/bbsbox-release.service) is separate from the unchanged development unit. Actual Debian signed installation, boot/reboot, private ownership, backup/isolated restore and pending-control fencing pass [CI](https://github.com/tellesus/BBSBOX/actions/runs/41555753176). This is not a production release or a G1/device pass.

When the offline rebuild report includes a demo worker digest, pass the matching rebuilt executable as `--game-worker` to `tools/release_candidate.py prepare`. The signed installer preserves it as a root-owned executable and boot preflight checks its signed digest. Enabling the game origin on the signed service requires the isolated appliance worker.

Verified application/source/license materials live under `/opt/bbsbox/releases/<release-id>`. A protected `/opt/bbsbox/maintenance/bbsbox-operator` copy owns stable local release commands. Root-only release-control state lives in `/var/lib/bbsbox-maintenance/release-control`, outside node snapshots. A durable pending ticket reserves the sequence before node initialization; the installer writes/synchronizes configuration, CA/secrets, unit and application selection, then completes control before enabling the service. Any failed/interrupted operation preserves its pending control for explicit recovery rather than silently restarting an incomplete installation. Automated first-install resumption remains open. Offline update/interruption recovery passes the Debian checkpoint below. Recorded rollback and extended startup exclusion pass the later checkpoint below.

Installation metadata is `bbsbox.install/v1`; public enrollment/network preparation and the runtime backup provider accept it alongside the existing development format. [The signed-node backup entry point](backup_node.py) shares cold-stop/ownership/restore logic while requiring the signed release layout and a successful installed boot gate. Backup keys and release keys retain separate roles; no release signing secret is installed. The normal node service stays unprivileged, including after backup/restart.

The isolated Debian fixture installs with Docker networking disabled, then boots/reboots the actual signed unit in private networking and checks HTTPS in both families, node/CA continuity and cold backup/isolated recovery blocking. It carries a valid next-sequence manifest without its signing key, reserves it, proves the pending gate prevents unit startup, records recovery with no app/data change, and resumes the original selection with the maximum sequence retained. This boot/policy fixture now also attempts actual migrations, matching rollback and process-interruption recovery as described below; the development ledger distinguishes verified checkpoints from pending work. Portable guards preserve existing node/control/application data and test signed public enrollment. The canonical roadmap and user instructions remain the scope authority.


## Offline foundation update and pending-operation recovery (M12)

Run [update_release.py](update_release.py) from matching prepared source on the signed Debian 13 amd64 appliance:

```sh
sudo python3 deploy/update_release.py apply --bundle /media/update/release --identity-file /private/operator-backup.key
sudo python3 deploy/update_release.py recover --operation SAVED_32_HEX_OPERATION --identity-file /private/operator-backup.key
```

Replace paths/operation ID with actual values. The operator backup identity is private and separate from node/configuration/application storage; no release signing key is needed on the appliance. The appliance never fetches code or dependencies. Candidate authentication, complete source/dependency/asset verification and bounded actual compiled-profile execution precede migration. The initial provider admits the same directly readable core schema, unchanged applied SQL for its supported identity, events, settings, discussions, mail, profiles, social, gatherings, groups and files owners. It changes installation metadata and application selection; TLS/address configuration migration and actual game checkpoint integration require their own consumers.

One global maintenance lock excludes backups and leaf renewal throughout update/recovery. An active service is stopped/drained. Before consuming a new sequence, the updater writes a private encrypted snapshot and proves it can decrypt/verify that exact inventory. Root-private `updates/<operation>/plan.json` records the old/new release IDs, source identity, snapshot inventory, sequence, revision fences and prior service state. Private key bytes never enter the plan. Pending control is durable before candidate SQL executes. Selection, unit and installation metadata are synchronized before completion. A previously inactive source stays inactive; a completed update can be resumed without restoring over later writes.

If migration fails, the coordinator restores the matching pre-update snapshot through the prior verified executable and a fresh node-signed continuation decision. It first stops the service and obtains the actual M02 source writer flock; a surviving migration process blocks replacement. The original lock inode survives all replacement attempts. Root release control blocks unit startup during partial work. Known core database/sidecar/blob entries are moved aside privately on the same data filesystem, fresh verified data is selected, and unchanged TLS/address configuration is checked. Node identity/CA and receipt history survive; missing post-snapshot writes remain unreconciled and client-key recovery stays separate. Unknown storage/configuration is preserved for reconciliation. The prior app/data selection becomes durable before failed-operation recovery is audited. The maximum consumed sequence/current authority stay intact.

After process loss, rerun `recover` with the saved operation ID and backup identity. A pending operation conservatively restores the matching snapshot again; an already completed operation keeps its completed selection. Preparation can reuse an already published, fully reverified candidate. Displaced data remains root-private under `/var/lib/bbsbox/.maintenance`; the encrypted pre-update archive/plan remains in the separate maintenance area. Update journals and their pre-update archives require separate operator retention. The explicit completed-update rollback below passes Debian confirmation.

Local tests use real compiled successor/failing assemblies, real existing node data, encrypted snapshots and isolated continuation. The [Debian checkpoint](https://github.com/tellesus/BBSBOX/actions/runs/41563596987) signs those exact offline-rebuilt assemblies under an ephemeral authority, proves data-dependent SQL failure restores the prior app/snapshot, kills an actual migration child group mid-SQL, verifies the pending unit gate, explicitly recovers, then migrates successfully and checks the upgraded registry's backup/reboot. All four jobs and 199 Rust tests pass. No release signing secret enters the guest. Later assertions below await their containing checkpoint; physical/device acceptance stays separate.


## Recorded rollback after a completed update

Use [rollback_release.py](rollback_release.py) to select the matching pre-update snapshot of the currently selected completed update:

```sh
sudo python3 deploy/rollback_release.py --operation COMPLETED_UPDATE_32_HEX_ID --identity-file /private/operator-backup.key --reason 'Reason for selecting the matching older application and snapshot'
```

The source update must still be selected, with its original protected plan/archive and a known completed historical release receipt. This is an explicit operation with its own new ID and `bbsbox.offline-rollback-plan/v1` journal. Before beginning historical recovery, it verifies the older snapshot/key/registry/configuration and also captures/decrypt-verifies current data into `displaced-before.age`. The current snapshot is retained for separate reconciliation; it is not merged into the selected older state. The existing compatible core/owner/configuration limits apply.

Separate release control records the reason/time/exact snapshot reference through its historical-recovery transition. The coordinator restores and signs a fresh continuation, durably selects the older application/data, then completes that recorded recovery. The current trusted release key and maximum sequence stay unchanged, including after a prior authorized key rotation. `update_release.py recover --operation <new-rollback-id> --identity-file <key>` resumes a pending recorded rollback; a retry after completed selection preserves later writes. `apply` exits 2 when an update attempt failed but recovered safely; a completed update and explicit successful recovery/rollback exit 0. Pending failures exit unsuccessfully with the saved operation ID.

The shipped signed unit now checks the global maintenance flock before startup, including preparation before any pending release ticket. `release-control-installed` performs the same protected installed-selection/public-registration inspection for coordinators already holding that lock, without acquiring it again. Helpers validate completion/repair known ownership while locked, release the global lock, then resume a previously active service. Root inspection does not open node SQLite; the ordinary unprivileged CLI checks the actual saved node key against installation registration. Normal serving requires prior explicit initialization, and installed initialization cannot recreate a missing/mismatched key. This keeps WAL sidecars owned by the service account.

The [extended Debian checkpoint](https://github.com/tellesus/BBSBOX/actions/runs/41566924144) passes all four jobs and 200 Rust tests. It confirms manual start refusal under maintenance with no pending ticket, renewal exclusion, nonzero failed-update status, safe completed/stale retries, actual release-key rotation, recorded older-release rollback and retained newer writes. Physical/phone acceptance remains separate.


## Fresh-appliance recovery and operator handover

Prepare fresh Debian 13 amd64 media with the existing offline dependencies. Keep a compatible signed release bundle and bootstrap verifier, the independently trusted release public key and minimum accepted sequence, and the saved node ID/CA fingerprint. Keep the encrypted snapshot and its private backup identity on separate operator media. The node archive carries node data and keys; the release authority and accepted sequence are supplied independently.

For planned handover, stop and disable the original `bbsbox.service` before taking the final [signed backup](backup_node.py). The backup helper leaves an inactive source stopped. Keep its public connection card and record the current `trusted_key` and `highest_sequence` from `release-control-state --directory /var/lib/bbsbox-maintenance/release-control`. The trusted key is a 32-byte JSON array; supply its lowercase hexadecimal encoding on the replacement. Hand over the saved node registration, release records, encrypted archive and separate backup identity. Leave the original node stopped. After unexpected source loss, use the last verified backup and independent operator release records.

```sh
sudo python3 deploy/recover_release.py \
  --bundle /media/release --verifier /private/bbsbox-operator \
  --public-key TRUSTED_RELEASE_PUBLIC_HEX --minimum-sequence SAVED_SEQUENCE \
  --archive /media/backup/node.age --identity-file /private/operator-backup.key \
  --node node:SAVED_NODE_DIGEST --source lost --reason 'Original appliance unavailable' \
  --enable
```

Use `--source retired` and a handover reason for a planned move. The explicit source disposition is recorded in the node-signed decision. The bootstrap rejects a release below the independently supplied sequence before probing that executable. It verifies the complete encrypted stream, matching node and known migration history, then migrates a disposable restored copy. Once ready, it records pending fresh release admission, copies closed data and protected configuration, checks identity as the unprivileged service user and durably completes selection before enabling the unit. Existing node, configuration, service overrides, operator control and previous bootstrap evidence are refused. If first recovery stops after admission, preserve the pending record for inspection and retry on fresh replacement media.

The replacement keeps the saved node ID, CA and neighborhood name. It issues a fresh loopback leaf under the saved CA and starts on localhost. Reapply the local network profile for the replacement hardware before opening visitor access; the original archive retains the previous generated network settings. Compare the new connection card with the saved CA fingerprint and node ID. The new appliance has explicitly bootstrapped release control outside the restored data. Newer writes remain unreconciled, and client key recovery uses the clients’ own exports.

The Debian fixture takes an older snapshot from a stopped source and uses the independently rotated release key to select a newer signed assembly. It checks wrong-node and below-floor refusal, preserves the original CA and data, applies real SQL, boots/reboots with trusted HTTPS, and reprovisions the local network. These installed source-loss checks passed in [Debian CI](https://github.com/tellesus/BBSBOX/actions/runs/41718200103).


## Daily backups on configured media

Mount writable local backup media outside the node's application, configuration and data. The first scheduler accepts ext4, XFS, Btrfs and F2FS. Setup validates the operator's public age recipient before registering the media. Keep its private identity on separate media and use it when testing a restore.

```sh
sudo python3 deploy/backup_schedule.py configure --mount /media/bbsbox-backup --recipient age1PUBLIC_RECIPIENT
sudo python3 deploy/backup_schedule.py run
sudo python3 deploy/backup_schedule.py install-timer
sudo systemctl daemon-reload
sudo systemctl enable --now bbsbox-backup.timer
```

The timer checks hourly and captures one backup per UTC day when the registered media is available. Setup registers one media directory and installs a private copy of the timer helpers. Repeating the same timer installation preserves its files. A detached drive is skipped until a later check. The service reports low space and retains the existing archives when a new snapshot cannot be admitted. The signed backup helper still handles service drain, protected capture and resumption.

Retention keeps the latest intact archive in each of the seven most recent backed-up days and the four most recent backed-up ISO weeks. A single archive can fill both slots. The appliance keeps receipts outside node snapshots and checks stored ciphertext against them before removing expired files. It preserves damaged, unindexed and linked entries. Pruning requires at least two intact retained archives. A copy of the encrypted media without its appliance receipts remains usable for recovery and stays outside automatic pruning.

[Debian CI](https://github.com/tellesus/BBSBOX/actions/runs/41718200103) started the backup service and timer on the recovered appliance, decrypted the resulting archive, checked the same-day skip and detached the test mount. Separate filesystem tests covered retention across calendar boundaries and preservation of the sole intact archive.
