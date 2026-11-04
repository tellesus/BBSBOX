# SPDX-License-Identifier: AGPL-3.0-or-later
"""Prepare the developer network profile in an inactive installed OS tree, before its next boot."""
import argparse
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
from urllib.parse import urlsplit

import enrollment
import network_profile
import local_tls

ROOT = Path(__file__).resolve().parent
MASK = ['NetworkManager.service', 'networking.service', 'systemd-network-generator.service', 'nftables.service', 'systemd-resolved.service', 'systemd-timesyncd.service', 'apt-daily.timer', 'apt-daily.service', 'apt-daily-upgrade.timer', 'apt-daily-upgrade.service', 'systemd-sysupdate.timer', 'systemd-sysupdate.service', 'fwupd-refresh.timer', 'fwupd-refresh.service', 'avahi-daemon.service', 'avahi-daemon.socket', 'dnsmasq.service']


def target(root, relative):
    path = root/relative
    if path.parent.resolve() != path.parent or not path.parent.is_relative_to(root):
        raise ValueError('Prepared paths must stay inside the nonsymlinked OS layout')
    return path


def preflight(root, profile):
    root = Path(root).absolute()
    if root != root.resolve() or root == Path('/') or not root.is_dir(): raise ValueError('Select an inactive prepared OS tree, never the current OS root')
    # A chroot path can name a running test OS; do not rewrite its service/TLS files.
    if Path('/proc').is_dir() and os.name == 'posix':
        expected = root.stat()
        for process in Path('/proc').iterdir():
            if not process.name.isdecimal(): continue
            try: current = (process/'root').stat()
            except OSError: continue
            if (current.st_dev, current.st_ino) == (expected.st_dev, expected.st_ino): raise ValueError('Prepared OS tree is currently running')
    installed = enrollment.read_json(target(root, 'etc/bbsbox/installation.json'))
    config = enrollment.read_json(target(root, 'etc/bbsbox/node.json'))
    node_id = installed.get('node_id', '')
    hostname = 'bbsbox-'+node_id.split(':')[-1][:16]+'.home.arpa'
    if installed.get('format') not in ('bbsbox.development-install/v1','bbsbox.install/v1') or not re.fullmatch(r'node:[a-f0-9]{64}',node_id) or installed.get('hostname') != hostname:
        raise ValueError('Expected a matching developer installation')
    profile = json.loads(json.dumps(profile))
    if profile.get('hostname') == 'auto': profile['hostname'] = hostname
    network_profile.validate(profile)
    if profile['hostname'] != hostname: raise ValueError('Network hostname must match the installed node')
    if config.get('bind') not in ['127.0.0.1:8443','[::1]:8443'] or config.get('origin') not in ['https://localhost:8443','https://127.0.0.1:8443','https://[::1]:8443'] or config.get('additional_binds'):
        raise ValueError('This first network setup requires the unchanged loopback installation')
    if (root/'etc/bbsbox/network').exists() or (root/'etc/bbsbox/.network-prepare-incomplete').exists():
        raise ValueError('Existing or interrupted network preparation requires inspection')
    for relative in ['etc/systemd/network', 'etc/sysctl.d', 'etc/systemd/system']:
        target(root, relative)
    if list((root/'etc/systemd/network').glob('*.network')): raise ValueError('Existing network configuration requires a dedicated prepared OS')
    for unit in ['bbsbox-boundary.service','bbsbox-dns.service']:
        path = target(root, 'etc/systemd/system/'+unit)
        if path.exists() or path.is_symlink(): raise ValueError('Existing local network unit is preserved')
    if (root/'opt/bbsbox/enrollment-loopback').exists(): raise ValueError('Previous enrollment archive already exists')
    if not (root/'usr/sbin/nft').is_file() or not (root/'usr/sbin/dnsmasq').is_file(): raise ValueError('Prepare local nftables/dnsmasq dependencies first')
    if not (root/'usr/lib/systemd/system/systemd-networkd.service').is_file(): raise ValueError('Prepare systemd-networkd first')
    _, fingerprint = enrollment.ca_fingerprint(root/'etc/bbsbox/tls/ca.pem')
    if installed.get('ca_sha256') != fingerprint: raise ValueError('Installed CA fingerprint mismatch')
    return root, installed, config, profile


def write_new(root, relative, data, mode=0o644, gid=0):
    path = target(root, relative); path.parent.mkdir(parents=True, exist_ok=True, mode=0o755)
    with path.open('xb') as stream:
        os.fchmod(stream.fileno(), mode); os.fchown(stream.fileno(), 0, gid)
        stream.write(data); stream.flush(); os.fsync(stream.fileno())
    return path


def dns_account(root):
    def read(file, name):
        return next((line.split(':') for line in (root/'etc'/file).read_text().splitlines() if line.split(':')[0]==name),None)
    account=read('passwd','bbsbox-dns')
    if account is None:
        subprocess.run(['useradd','--root',str(root),'--system','--user-group','--no-create-home','--home-dir','/nonexistent','--shell','/usr/sbin/nologin','bbsbox-dns'],check=True,capture_output=True,timeout=20)
        account=read('passwd','bbsbox-dns')
    group=read('group','bbsbox-dns')
    if not account or not group or int(account[2])==0 or int(account[3])==0 or account[3]!=group[2] or account[5]!='/nonexistent' or account[6]!='/usr/sbin/nologin':
        raise ValueError('Existing DNS account does not match the dedicated unprivileged service')


def configure(root, profile):
    if os.name != 'posix' or os.geteuid() != 0: raise ValueError('Prepare an inactive Debian OS tree as root')
    root, installed, config, profile = preflight(root, profile)
    config_path = root/'etc/bbsbox/node.json'; gid = config_path.stat().st_gid
    marker = write_new(root, 'etc/bbsbox/.network-prepare-incomplete', b'Network preparation must finish before boot.\n', 0o600)
    dns_account(root)
    # This guard remains if preparation is interrupted. Local services cannot start
    # with an incomplete mix of configuration/certificate/network files.
    write_new(root, 'etc/systemd/system/bbsbox.service.d/30-local-network.conf', b'[Unit]\nConditionPathExists=!/etc/bbsbox/.network-prepare-incomplete\nRequires=bbsbox-boundary.service\nAfter=bbsbox-boundary.service systemd-networkd.service\nStartLimitIntervalSec=0\n')
    files = network_profile.render(profile)
    for name, content in files.items(): write_new(root, 'etc/bbsbox/network/'+name, content.encode())
    for name, content in files.items():
        if name.startswith('networkd/'): write_new(root, 'etc/systemd/network/'+Path(name).name, content.encode())
    write_new(root, 'etc/sysctl.d/90-bbsbox-local.conf', files['90-bbsbox-local.conf'].encode())
    for unit in ['bbsbox-boundary.service', 'bbsbox-dns.service']:
        write_new(root, 'etc/systemd/system/'+unit, (ROOT/'systemd'/unit).read_bytes())
    write_new(root, 'etc/systemd/system/systemd-networkd.service.d/30-bbsbox-boundary.conf', b'[Unit]\nRequires=bbsbox-boundary.service\nAfter=bbsbox-boundary.service\n')
    for unit in MASK:
        destination = target(root, 'etc/systemd/system/'+unit)
        if destination.exists() or destination.is_symlink():
            if destination.is_symlink() and os.readlink(destination) == '/dev/null': continue
            raise ValueError('Existing local service configuration is preserved: '+unit)
        destination.symlink_to('/dev/null')
    addresses = json.loads(files['origins.json'])
    game = addresses.pop('game_origin', None)
    hosts = [address.split('/')[0] for interface in profile['interfaces'] for address in interface['addresses']]
    if game:
        hosts.append(urlsplit(game).hostname)
    leaf = local_tls.issue(root/'etc/bbsbox/tls/ca.pem', root/'etc/bbsbox/tls/ca-key.pem', [profile['hostname'], *hosts], root/'etc/bbsbox/network')
    write_new(root, 'etc/bbsbox/network/tls/server.pem', leaf['certificate'], 0o644, gid)
    write_new(root, 'etc/bbsbox/network/tls/server-key.pem', leaf['key'], 0o640, gid)
    config.update(addresses, certificate='/etc/bbsbox/network/tls/server.pem', certificate_key='/etc/bbsbox/network/tls/server-key.pem')
    prepared = write_new(root, 'etc/bbsbox/node.network.json', (json.dumps(config, indent=2)+'\n').encode(), 0o640, gid)
    enrollment.export(root/'etc/bbsbox/installation.json', prepared, root/'etc/bbsbox/tls/ca.pem', root/'etc/bbsbox/network/enrollment')
    old_card = target(root, 'opt/bbsbox/enrollment'); archived = target(root, 'opt/bbsbox/enrollment-loopback')
    if old_card.exists(): old_card.rename(archived)
    shutil.copytree(root/'etc/bbsbox/network/enrollment', old_card)
    resolver = target(root, 'etc/resolv.conf')
    # Replace only this inactive tree's resolver leaf; never follow a symlink outside it.
    if resolver.exists() or resolver.is_symlink(): resolver.unlink()
    write_new(root, 'etc/resolv.conf', b'nameserver 127.0.0.1\nnameserver ::1\noptions timeout:1 attempts:1\n')
    for unit, source in [('bbsbox-boundary.service','/etc/systemd/system/bbsbox-boundary.service'), ('bbsbox-dns.service','/etc/systemd/system/bbsbox-dns.service'), ('systemd-networkd.service','/usr/lib/systemd/system/systemd-networkd.service')]:
        destination = target(root, 'etc/systemd/system/multi-user.target.wants/'+unit)
        destination.parent.mkdir(parents=True, exist_ok=True)
        if destination.exists() or destination.is_symlink(): raise ValueError('Existing activation is preserved: '+unit)
        destination.symlink_to(source)
    prepared.replace(config_path)
    marker.unlink()
    return dict(format='bbsbox.development-network-install/v1', node_id=installed['node_id'], hostname=profile['hostname'], ca_sha256=installed['ca_sha256'], activated=False, next_boot=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__); parser.add_argument('--root', type=Path, required=True); parser.add_argument('--profile', type=Path, required=True); parser.add_argument('--development', action='store_true', required=True); args = parser.parse_args()
    print(json.dumps(configure(args.root, enrollment.read_json(args.profile)), indent=2))


if __name__ == '__main__': main()
