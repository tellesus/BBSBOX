# SPDX-License-Identifier: AGPL-3.0-or-later
"""Prepare an explicit local-only network profile; never change the current host."""
import argparse
import ipaddress
import json
from pathlib import Path
import re

FORMAT = 'bbsbox.local-network/v1'
PRIVATE = [ipaddress.ip_network(value) for value in ['10.0.0.0/8', '172.16.0.0/12', '192.168.0.0/16', 'fc00::/7']]


def validate(profile):
    if set(profile) != {'format', 'hostname', 'https_port', 'interfaces'} or profile['format'] != FORMAT:
        raise ValueError('Unknown local-network profile fields/format')
    if not re.fullmatch(r'bbsbox-[a-f0-9]{16}\.home\.arpa', profile['hostname']):
        raise ValueError('Use the installation node-ID-suffixed home.arpa name')
    if type(profile['https_port']) is not int or not 1024 <= profile['https_port'] <= 65535:
        raise ValueError('HTTPS requires an unprivileged port')
    interfaces = profile['interfaces']
    if not isinstance(interfaces, list) or not 1 <= len(interfaces) <= 8:
        raise ValueError('Declare one to eight local interfaces')
    names = set(); networks = []; addresses = set(); clients = 0
    for interface in interfaces:
        if set(interface) != {'name', 'role', 'addresses'}:
            raise ValueError('Unknown interface fields')
        name = interface['name']
        if not isinstance(name, str) or not re.fullmatch(r'[a-zA-Z0-9][a-zA-Z0-9_-]{0,14}', name) or name == 'lo' or name in names:
            raise ValueError('Use unique literal Linux interface names, excluding loopback')
        names.add(name)
        if interface['role'] not in ['client', 'peer']:
            raise ValueError('Interface role must be client or explicitly opted-in peer')
        clients += interface['role'] == 'client'
        values = interface['addresses']
        if not isinstance(values, list) or not 1 <= len(values) <= 2:
            raise ValueError('Declare at most one static address per IP family per interface')
        families = set()
        for value in values:
            if not isinstance(value, str): raise ValueError('Address must be CIDR text')
            address = ipaddress.ip_interface(value)
            if value != str(address) or address.version in families or address.ip in addresses:
                raise ValueError('Use unique canonical CIDRs and one address per family')
            families.add(address.version); addresses.add(address.ip)
            if not any(address.network.version == private.version and address.network.subnet_of(private) for private in PRIVATE):
                raise ValueError('Only explicit RFC1918/ULA local networks are supported')
            if address.network.num_addresses < 4 or address.ip == address.network.network_address or (address.version == 4 and address.ip == address.network.broadcast_address):
                raise ValueError('Use a usable static host address and a local subnet')
            if any(address.version == old.version and address.network.overlaps(old) for old in networks):
                raise ValueError('Local interface subnets must not overlap')
            networks.append(address.network)
    if not clients: raise ValueError('At least one local client interface is required')
    if len(addresses)>8: raise ValueError('At most eight advertised local addresses are supported')
    return profile


def render(profile):
    validate(profile)
    port = profile['https_port']; hostname = profile['hostname']
    inbound = ['iifname "lo" ip saddr 127.0.0.0/8 ip daddr 127.0.0.0/8 accept', 'iifname "lo" ip6 saddr ::1 ip6 daddr ::1 accept', 'ct state invalid drop']
    outbound = ['oifname "lo" ip saddr 127.0.0.0/8 ip daddr 127.0.0.0/8 accept', 'oifname "lo" ip6 saddr ::1 ip6 daddr ::1 accept', 'ct state invalid drop']
    files = {}; hosts = []; listeners = ['127.0.0.1', '::1']
    for interface in profile['interfaces']:
        name = interface['name']
        addresses = [ipaddress.ip_interface(value) for value in interface['addresses']]
        # Accept established replies only after matching the allowed interface/address pair.
        # A pre-existing WAN connection must not bypass the destination boundary.
        for address in addresses:
            family = 'ip' if address.version == 4 else 'ip6'
            net = address.network; own = address.ip
            incoming = f'iifname "{name}" {family} saddr {net} {family} daddr {own}'
            outgoing = f'oifname "{name}" {family} saddr {own} {family} daddr {net}'
            inbound += [f'{incoming} ct state established,related accept', f'{incoming} tcp dport {{ 53, {port} }} accept', f'{incoming} udp dport 53 accept']
            outbound.append(f'{outgoing} ct state established,related accept')
            if interface['role'] == 'peer': outbound.append(f'{outgoing} tcp dport {port} accept')
            hosts.append(str(own)); listeners.append(str(own))
        # IPv6 neighbor discovery is link-local control traffic, never a WAN route grant.
        if any(address.version == 6 for address in addresses):
            for lines, direction in [(inbound, 'iifname'), (outbound, 'oifname')]:
                lines.append(f'{direction} "{name}" ip6 saddr {{ ::/128, fe80::/10, {next(a.network for a in addresses if a.version == 6)} }} ip6 daddr {{ fe80::/10, ff02::/16, {next(a.network for a in addresses if a.version == 6)} }} ip6 hoplimit 255 icmpv6 type {{ nd-neighbor-solicit, nd-neighbor-advert }} accept')
        link_local = 'ipv6' if any(a.version == 6 for a in addresses) else 'no'
        files[f'networkd/30-bbsbox-{name}.network'] = f'[Match]\nName={name}\n\n[Network]\nDHCP=no\nIPv6AcceptRA=no\nLinkLocalAddressing={link_local}\nLLMNR=no\nMulticastDNS=no\nDNSDefaultRoute=no\nIPForward=no\n' + ''.join(f'Address={a}\n' for a in addresses)
    chains = []
    for name, rules in [('input', inbound), ('output', outbound), ('forward', [])]:
        chains.append(f' chain {name} {{\n  type filter hook {name} priority filter; policy drop;\n' + ''.join(f'  {rule}\n' for rule in rules) + '  counter drop\n }\n')
    files['bbsbox.nft'] = '# Prepared for an isolated appliance. Load before bringing up external interfaces.\ntable inet bbsbox_boundary {\n' + ''.join(chains) + '}\n'
    files['dnsmasq.conf'] = '# Start with --conf-file pointing only to this file. No upstream or extra conf-dir.\nno-resolv\nno-poll\nno-hosts\nbind-interfaces\nlocal=/home.arpa/\naddress=/#/\nlisten-address=' + ','.join(listeners) + '\n' + ''.join('host-record='+hostname+','+host+'\n' for host in hosts)
    files['90-bbsbox-local.conf'] = 'net.ipv4.ip_forward=0\nnet.ipv6.conf.all.forwarding=0\nnet.ipv6.conf.default.forwarding=0\nnet.ipv6.conf.all.accept_ra=0\nnet.ipv6.conf.default.accept_ra=0\nnet.ipv6.conf.all.autoconf=0\nnet.ipv6.conf.default.autoconf=0\n'
    binds = [f'[{ip}]:{port}' if ':' in ip else f'{ip}:{port}' for ip in hosts]
    files['origins.json'] = json.dumps(dict(bind=binds[0], additional_binds=binds[1:], origin=f'https://{hostname}:{port}', additional_origins=['https://'+bind for bind in binds]), indent=2)+'\n'
    files['profile.json'] = json.dumps(profile, indent=2)+'\n'
    return files


def prepare(profile, output):
    files = render(profile)
    output = Path(output).absolute()
    if output != output.resolve(): raise ValueError('Output must not traverse symlinks')
    output.mkdir(parents=True, exist_ok=False)
    for name, value in files.items():
        path = output/name; path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(value, encoding='utf-8')
    return sorted(files)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profile', type=Path, required=True); parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    with args.profile.open('rb') as source:
        data = source.read(65537)
    if len(data)>65536: raise ValueError('Profile exceeds 64 KiB')
    print(json.dumps(dict(prepared=prepare(json.loads(data), args.output), activated=False)))


if __name__ == '__main__': main()
