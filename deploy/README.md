# M01 developer installation fixture

Reference for [M01](../docs/development/modules/M01-offline-appliance.md); the canonical roadmap and user instructions own product scope. This fixture exercises first installation on an isolated Debian 13 amd64 developer target. M12 owns authenticated production releases/updates. M01 B/C and physical appliance/phone acceptance remain pending.

## Prepared inputs and first install

Prepare Python 3, OpenSSL, the standard account utilities, systemd and the binary's local runtime dependencies on the developer target. The installer checks those tools and never invokes a package manager or downloads material. Build and verify the [development payload](../tools/runtime_bundle.py) on the developer/build machine, then transfer it and these project scripts locally.

On the isolated test target, from a checkout of the matching source:

```sh
sudo python3 deploy/install_development.py --bundle /path/to/runtime-bundle --development --enable
```

`--enable` installs the unit's next-boot link; it does not start a service. This first-install fixture refuses existing node configuration, TLS material, activated binaries/units or nonempty state. It does not replace a live installation or initialize over community data. The full offline network/DNS profile is a later M01 slice; this initial configuration binds loopback only.

The installer verifies the exact payload inventory, creates an unprivileged `bbsbox` OS account, installs immutable runtime files under `/opt/bbsbox/development/<manifest-sha256>`, and writes the [unit](systemd/bbsbox.service), `/etc/bbsbox/node.json` and private `/var/lib/bbsbox` state. Offline initialization obtains the stable node ID before certificate generation. Node metadata is not a portable member key/recovery kit.

Each node gets its own local CA and a leaf for localhost, both loopback IPs and a node-ID-suffixed name under [home.arpa](https://www.rfc-editor.org/rfc/rfc8375.html). The CA private key is root-only; the service can read its leaf key and configuration but cannot replace them. The resulting public CA fingerprint/installation metadata is recorded locally. No trust store or peer trust is automatically installed. Local DNS/address cards and visitor CA enrollment remain M01 B work.

Before enabling the unit, installation checks those read permissions as the actual service account, including denial of CA-private-key access. Directory modes are set explicitly after the private provisioning umask.

The installer also writes a public connection card under `/opt/bbsbox/enrollment`: `connect.html`, `node.json` and `ca.pem`. Copy those three files to local visitor media or print the card. The card lists configured local addresses, public node ID and the CA certificate's SHA-256 fingerprint. Compare that fingerprint through a separate SysOp-supplied copy before enrolling trust. The export includes no private key or database/configuration path and changes no trust store. Device-specific enrollment still needs the planned phone milestone tests.

To prepare a fresh card after a configuration change, run `sudo python3 deploy/enrollment.py --output /path/to/new-card-directory` from the matching source checkout. The tool checks that the CA matches the installation record and that addresses are local; an existing output directory is preserved and rejected. CA fingerprint checking here uses local certificate/time checks without online lookups.

## Actual Debian service verification

[The CI image recipe](ci/Dockerfile) prepares dependencies from the official Debian trixie image on a developer/build network. Its installation instruction then runs with networking disabled. The image is a developer test tool; production does not require Docker or a registry.

[The context/export helper](ci/prepare.py) copies only the selected payload/scripts into a fresh workspace context. Before extracting the exported OS, it validates archive paths, symlink ancestry, hardlink targets and entry types. The target must be a fresh directory beneath repository `work/`.

[The boot check](ci/boot_service.py) starts the prepared tree with actual systemd under [systemd-nspawn](https://www.freedesktop.org/software/systemd/man/latest/systemd-nspawn.html), in a separate PID/network namespace containing only loopback and no gateway. It identifies that exact OS root before entering it, checks the real unit/cgroup/unprivileged process and verified HTTPS, restarts the unit, switches the fixture to IPv6 and boots the OS again with the same node identity. Shutdown signals only the child manager it started. [nsenter](https://man7.org/linux/man-pages/man1/nsenter.1.html) supplies access to that validated test namespace.

These target checks must pass in CI before being called evidence. Local Windows checks cover installation-preservation/archive guards and runtime behavior; they do not execute Debian installation or systemd. Reports/logs are developer evidence, with physical reference-machine, router/radio and phone checks still separate. The remaining signed-release, coordinated backup and recovery activation work stays with M12.

## Local network profile preparation

[The profile preparer](network_profile.py) takes an explicit [local-interface configuration](network.example.json) and writes a fresh output directory without activating anything:

```sh
python3 deploy/network_profile.py --profile deploy/network.example.json --output work/local-network
```

Replace the example hostname with the installation's node-ID-suffixed name and select actual interface names/static addresses. Networks must be nonoverlapping RFC1918 IPv4 or ULA IPv6 subnets. This profile uses the isolated router for client address assignment; it does not run DHCP or provide internet routing. Peer interfaces require an explicit `peer` role; the shipped example has none. Application federation still requires separately approved peers.

Prepared files include static networkd settings without DHCP/default gateway/router advertisements, forwarding-off sysctls, a DNS-only dnsmasq configuration, exact DNS/IP runtime origins/listeners, and an inet nftables boundary. Rules bind each allowed subnet to its named interface and node address. Clients can reach HTTPS/local DNS; new node-originated HTTPS is restricted to explicitly configured peer interfaces. Established replies are permitted only inside those same interface/address boundaries, so an already established WAN connection receives no exemption. Other input/output and all forwarding drop in both IP families. IPv6 neighbor discovery is limited to configured local links.

The DNS process must use only the generated `--conf-file`, without additional configuration directories or upstream arguments. It serves the exact local hostname using [dnsmasq's](https://thekelleys.org.uk/dnsmasq/docs/dnsmasq-man.html) local-only mode, ignores resolver/hosts files and has no upstream server. The [nftables reference](https://netfilter.org/projects/nftables/manpage.html) defines the inet hooks, interface matches and state handling used here.

[The isolated boundary test](ci/network_boundary.py) creates simulated client, peer and WAN interfaces/default routes, checks both IP families, local DNS/HTTPS-port traffic, peer initiation, blocked client initiation/forwarding and blocked new/already-established WAN traffic. It captures outgoing IP frames on the simulated WAN and saves counters. These assertions require passing Linux CI. They test the generated rules/DNS; boot-time host activation, background-service suppression, LAN certificate issuance/enrollment, router instructions and physical acceptance remain M01 B/C work. Do not apply this candidate as a complete appliance installer yet.

## Inactive Debian tree network candidate

[The next-boot preparer](configure_network.py) configures an inactive developer OS tree after the first install, using the same node identity and local CA. It refuses the running OS root, a tree currently used by a process, existing local network files and interrupted preparation. It does not start any service or modify community storage. Set the input profile's `hostname` to `auto` to derive the installed node name, or use that exact name explicitly:

```sh
sudo python3 deploy/configure_network.py --root work/debian-root --profile work/network-install.json --development
```

Prepare Debian's nftables, dnsmasq-base, iproute2 and systemd dependencies before isolating the image. This targets the dedicated minimal Debian tree, not migration of an existing workstation/router. The preparer writes static networkd files, a firewall-before-network dependency, dedicated DNS service, local resolver settings and masks for automatic online package/time/update/discovery services. LAN leaf certificates are signed by the existing CA; the current public card is regenerated and the loopback card is retained as `enrollment-loopback`. Private CA/node/member keys are not replaced. An incomplete marker blocks runtime startup after an interrupted preparation; inspect and finish/rebuild that candidate before boot.

[The network boot fixture](ci/boot_network.py) uses actual Debian systemd/networkd/nftables/dnsmasq and the real HTTPS runtime over a simulated visitor link. It checks DNS, both address families, node/CA continuity, disabled forwarding/online services and a second OS boot without peers. Those assertions await passing CI. This is separate from physical router/radio, visitor device enrollment and complete boot packet-capture acceptance. The generated [networkd settings](https://www.freedesktop.org/software/systemd/man/latest/systemd.network.html) use static addresses without DHCP, router advertisements or a default route.

## Local diagnosis candidate

Run `sudo python3 deploy/diagnose.py` from the matching source checkout on the Debian node. It reads service state, local routes, firewall chain policies, online-service masks, certificate chain/expiry and the current UTC clock. It reports attention when a required check fails and returns a failing exit status. It never requests a DNS lookup or contacts a server; its JSON includes selected public node details and local diagnostic results. It does not read private key contents. CI runs this tool inside the prepared network guest; its integration awaits that check.

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

Two real-certificate tests pass locally: repeat issuance changes the server key/serial, preserves CA bytes and serial-file state, verifies intended names and rejects a wrong IP/public or injected names. CI additionally renews inside the actual guest, checks the running process still serves the prior leaf, restarts it and checks the new certificate fingerprint plus node/CA continuity. Guest integration remains pending until that run passes.

## Prepared-media reinstall check

Use fresh developer OS media with the prepared dependencies, matching source and verified runtime payload. Retain the original node's storage/configuration/TLS media intact. Follow first installation on the fresh media and verify its local status page, node ID, CA fingerprint and service restart. It creates an independent test node; record that its node ID differs from the retained source. Running first installation against existing state must fail and leave that state unchanged; the portable preservation tests cover this refusal. The coordinated original-node backup/restore/activation path is M12's next module. Reference-hardware execution remains an acceptance item.
