# SPDX-License-Identifier: AGPL-3.0-or-later
"""First-install fixture for an isolated Debian 13 developer machine; M12 owns release/update trust."""
import argparse
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import unicodedata

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT/'tools'))
import runtime_bundle as payload
import enrollment


@dataclass(frozen=True)
class Layout:
    root: Path

    @property
    def config(self): return self.root/'etc/bbsbox'
    @property
    def data(self): return self.root/'var/lib/bbsbox'
    @property
    def application(self): return self.root/'opt/bbsbox'
    @property
    def unit(self): return self.root/'etc/systemd/system/bbsbox.service'


def check_fresh(layout):
    root = layout.root.resolve()
    for path in [layout.config, layout.data, layout.application, layout.unit.parent]:
        if path.absolute() != path.resolve() or not path.resolve().is_relative_to(root):
            raise ValueError('Installation paths must stay in their intended nonsymlinked layout')
    for path in [layout.config/'node.json', layout.config/'tls', layout.application/'current', layout.unit]:
        if path.exists() or path.is_symlink():
            raise ValueError(f'First install refuses existing configuration/activation: {path}')
    if layout.data.exists() and any(layout.data.iterdir()):
        raise ValueError('First install refuses a nonempty node state directory')


def run(arguments, **kwargs):
    return subprocess.run(arguments, check=True, capture_output=True, **kwargs).stdout


def platform_preflight():
    if sys.platform != 'linux' or os.geteuid() != 0 or platform.machine() != 'x86_64':
        raise ValueError('Use an isolated Debian 13 amd64 developer machine as root')
    fields = {}
    import shlex
    for line in Path('/etc/os-release').read_text().splitlines():
        if '=' in line:
            key, value = line.split('=', 1)
            fields[key] = shlex.split(value)[0] if value else ''
    if fields.get('ID') != 'debian' or fields.get('VERSION_ID') != '13':
        raise ValueError('This development installation targets Debian 13')
    for command in ['openssl', 'runuser', 'useradd', 'systemd-analyze']:
        if not shutil.which(command):
            raise ValueError(f'Prepare the local dependency first: {command}')


def service_account(name='bbsbox'):
    import pwd
    import grp
    home='/var/lib/'+name
    try:
        user = pwd.getpwnam(name)
    except KeyError:
        run(['useradd', '--system', '--user-group', '--no-create-home', '--home-dir', home, '--shell', '/usr/sbin/nologin', name])
        user = pwd.getpwnam(name)
    group = grp.getgrnam(name)
    if user.pw_uid == 0 or group.gr_gid == 0 or user.pw_gid != group.gr_gid or user.pw_dir != home or user.pw_shell not in ['/usr/sbin/nologin', '/bin/false']:
        raise ValueError('Existing '+name+' OS account does not match the service layout')
    return user.pw_uid, group.gr_gid


def write_new(path, data, mode, uid, gid):
    with path.open('xb') as target:
        target.write(data)
        target.flush()
        os.fsync(target.fileno())
    os.chmod(path, mode)
    os.chown(path, uid, gid)


def tls_material(directory, node_id, group):
    short = node_id.split(':', 1)[1][:16]
    hostname = f'bbsbox-{short}.home.arpa'
    directory.mkdir(mode=0o750)
    os.chown(directory, 0, group)
    directory.chmod(0o750)
    with tempfile.TemporaryDirectory(prefix='tls-prepare-', dir=directory.parent) as temporary:
        work = Path(temporary).resolve()
        if not work.is_relative_to(directory.parent.resolve()):
            raise ValueError('TLS staging escaped the installation layout')
        (work/'openssl.cnf').write_text('[req]\ndistinguished_name=dn\nprompt=no\n[dn]\nCN=BBSBOX\n', encoding='ascii')
        def openssl(*args): return run(['openssl', *args], cwd=work)
        openssl('req', '-config', 'openssl.cnf', '-x509', '-newkey', 'rsa:3072', '-sha256', '-nodes', '-days', '3650', '-subj', f'/CN=BBSBOX {short} local CA', '-addext', 'basicConstraints=critical,CA:TRUE,pathlen:0', '-addext', 'keyUsage=critical,keyCertSign,cRLSign', '-keyout', 'ca-key.pem', '-out', 'ca.pem')
        openssl('req', '-config', 'openssl.cnf', '-new', '-newkey', 'rsa:3072', '-nodes', '-subj', f'/CN={hostname}', '-keyout', 'server-key.pem', '-out', 'server.csr')
        (work/'leaf.ext').write_text(f'basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=DNS:{hostname},DNS:localhost,IP:127.0.0.1,IP:::1\n', encoding='ascii')
        openssl('x509', '-req', '-in', 'server.csr', '-CA', 'ca.pem', '-CAkey', 'ca-key.pem', '-CAcreateserial', '-days', '365', '-sha256', '-extfile', 'leaf.ext', '-out', 'server.pem')
        for option,value in [('-verify_hostname',hostname),('-verify_hostname','localhost'),('-verify_ip','127.0.0.1'),('-verify_ip','::1')]:
            openssl('verify','-CAfile','ca.pem',option,value,'server.pem')
        for name, mode, gid in [('ca-key.pem', 0o600, 0), ('server-key.pem', 0o640, group), ('ca.pem', 0o644, 0), ('server.pem', 0o644, 0)]:
            write_new(directory/name, (work/name).read_bytes(), mode, 0, gid)
        der = openssl('x509', '-in', 'ca.pem', '-outform', 'DER')
    fingerprint = ':'.join(re.findall('..', hashlib.sha256(der).hexdigest().upper()))
    return hostname, fingerprint


def install(bundle, name, enable=False):
    information = payload.verify(bundle)
    platform_preflight()
    layout = Layout(Path('/'))
    check_fresh(layout)
    if not name.strip() or len(name)>64 or any(unicodedata.category(char)=='Cc' for char in name):
        raise ValueError('Invalid public node name')
    uid, gid = service_account()
    previous_umask = os.umask(0o077)
    try:
        layout.application.mkdir(parents=True, exist_ok=True, mode=0o755)
        layout.application.chmod(0o755)
        releases = layout.application/'development'
        releases.mkdir(mode=0o755, exist_ok=True)
        releases.chmod(0o755)
        release = releases/information['manifest_sha256']
        release.mkdir(mode=0o755)
        release.chmod(0o755)
        for member in sorted(payload.MEMBERS | {'manifest.json'}):
            destination = release/member
            destination.parent.mkdir(parents=True, exist_ok=True, mode=0o755)
            destination.parent.chmod(0o755)
            payload.copy_bounded(Path(bundle)/member, destination)
            destination.chmod(0o755 if member=='bbsbox-node' else 0o644)
        payload.verify(release)
        layout.config.mkdir(mode=0o750, parents=True, exist_ok=True)
        os.chown(layout.config, 0, gid)
        layout.config.chmod(0o750)
        layout.data.mkdir(mode=0o700, parents=True, exist_ok=True)
        os.chown(layout.data, uid, gid)
        layout.data.chmod(0o700)
        config = dict(format='bbsbox.node-config/v1', name=name, bind='127.0.0.1:8443', origin='https://localhost:8443', database=str(layout.data/'node.sqlite'), certificate=str(layout.config/'tls/server.pem'), certificate_key=str(layout.config/'tls/server-key.pem'))
        config_path = layout.config/'node.json'
        write_new(config_path, payload.encoded(config), 0o640, 0, gid)
        initialized = json.loads(run(['runuser', '-u', 'bbsbox', '--', str(release/'bbsbox-node'), '--config', str(config_path), '--initialize']))
        node_id = initialized.get('node_id', '')
        if initialized.get('format')!='bbsbox.node-init/v1' or not re.fullmatch(r'node:[a-f0-9]{64}', node_id):
            raise ValueError('Unexpected initialization response')
        hostname, fingerprint = tls_material(layout.config/'tls', node_id, gid)
        for needed in [config_path,layout.config/'tls/server.pem',layout.config/'tls/server-key.pem']:
            run(['runuser','-u','bbsbox','--','/usr/bin/test','-r',str(needed)])
        run(['runuser','-u','bbsbox','--','/usr/bin/test','!','-r',str(layout.config/'tls/ca-key.pem')])
        layout.unit.parent.mkdir(parents=True, exist_ok=True)
        write_new(layout.unit, (release/'bbsbox.service').read_bytes(), 0o644, 0, 0)
        (layout.application/'current').symlink_to(release, target_is_directory=True)
        run(['systemd-analyze', 'verify', '--man=no', str(layout.unit)])
        if enable:
            wanted = layout.unit.parent/'multi-user.target.wants'
            if not wanted.exists():
                wanted.mkdir(mode=0o755)
                wanted.chmod(0o755)
            (wanted/layout.unit.name).symlink_to(layout.unit)
        result = dict(format='bbsbox.development-install/v1', node_id=node_id, hostname=hostname, origin=config['origin'], ca_sha256=fingerprint, payload_sha256=information['manifest_sha256'], unit_enabled=enable)
        write_new(layout.config/'installation.json', payload.encoded(result), 0o640, 0, gid)
        public = layout.application/'enrollment'
        enrollment.export(layout.config/'installation.json', config_path, layout.config/'tls/ca.pem', public)
        public.chmod(0o755)
        for path in public.iterdir(): path.chmod(0o644)
        return result
    finally:
        os.umask(previous_umask)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bundle', type=Path, required=True)
    parser.add_argument('--name', default='BBSBOX')
    parser.add_argument('--development', action='store_true', required=True)
    parser.add_argument('--enable', action='store_true', help='Enable for the next isolated test boot; never starts a service here')
    args = parser.parse_args()
    print(json.dumps(install(args.bundle.resolve(), args.name, args.enable), indent=2))


if __name__ == '__main__':
    main()
