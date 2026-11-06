# SPDX-License-Identifier: AGPL-3.0-or-later
"""Cold backup of the installed Debian development node; restore stays isolated/recovery-blocked."""
import argparse
from contextlib import contextmanager
import json
import os
from pathlib import Path
import stat
import subprocess

GAME_STATE=Path('/var/lib/bbsbox-game')
GAME_SOCKET='bbsbox-game-worker.socket'


def regular(path, owners):
    metadata=path.lstat()
    if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid not in owners:
        raise ValueError('Expected an owned regular maintenance file')


def repair_ownership(database, uid, gid):
    # Known SQLite/lock names only. No recursive ownership changes or followed links.
    for suffix in ['', '-wal', '-shm', '-journal', '.writer.lock']:
        path=Path(str(database)+suffix)
        try: descriptor=os.open(path,os.O_RDONLY|os.O_NOFOLLOW)
        except FileNotFoundError: continue
        try:
            metadata=os.fstat(descriptor)
            if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid not in [0,uid]:
                raise ValueError('Unexpected node database/sidecar owner or type')
            os.fchown(descriptor,uid,gid);os.fchmod(descriptor,0o600);os.fsync(descriptor)
        finally: os.close(descriptor)


def run(arguments, timeout=120):
    try:return subprocess.run(arguments,check=True,capture_output=True,timeout=timeout)
    except subprocess.CalledProcessError as error:
        raise RuntimeError('Local maintenance command failed: '+error.stderr[-8192:].decode(errors='replace')) from error


def prepare_stage(path):
    try:path.mkdir(mode=0o700)
    except FileExistsError:pass
    metadata=path.lstat()
    if not stat.S_ISDIR(metadata.st_mode) or metadata.st_uid!=0 or metadata.st_mode&0o077:
        raise ValueError('Maintenance staging must be a private root-owned directory')


def game_state_retained(config):
    try: metadata=GAME_STATE.lstat()
    except FileNotFoundError: return bool(config.get('game_origin'))
    if not stat.S_ISDIR(metadata.st_mode):
        raise ValueError('Expected a direct appliance game state directory')
    return bool(config.get('game_origin')) or any(path.name!='state.sqlite.lock' for path in GAME_STATE.iterdir())


@contextmanager
def checkpoint_socket(required):
    temporary=False
    try:
        if required:
            status=subprocess.run(['/usr/bin/systemctl','is-active',GAME_SOCKET],capture_output=True,timeout=20).stdout.strip()
            if status not in (b'active',b'inactive'):
                raise ValueError('Expected the installed game worker socket')
            if status==b'inactive':
                run(['/usr/bin/systemctl','start',GAME_SOCKET]);temporary=True
        yield
    finally:
        if temporary:run(['/usr/bin/systemctl','stop',GAME_SOCKET])


def backup(destination, recipient, release=False):
    if os.name!='posix' or os.geteuid()!=0:raise ValueError('Use the Debian development node as root')
    import fcntl
    import pwd
    import enrollment
    import install_development
    install_development.platform_preflight()
    destination=Path(destination).absolute()
    configuration=Path('/etc/bbsbox');database=Path('/var/lib/bbsbox/node.sqlite')
    if destination!=destination.resolve() or any(destination.is_relative_to(path) for path in [configuration,database.parent,GAME_STATE]):
        raise ValueError('Use fresh backup media outside configuration and node storage')
    if destination.exists():raise ValueError('Backup output already exists')
    if not isinstance(recipient,str) or len(recipient)>1024 or not recipient.startswith('age1') or any(c.isspace() for c in recipient):
        raise ValueError('Use the operator backup recipient')
    user=pwd.getpwnam('bbsbox')
    if user.pw_uid==0 or user.pw_gid==0:raise ValueError('Expected the unprivileged bbsbox account')
    for directory in [configuration,database.parent]:
        if directory!=directory.resolve() or not directory.is_dir():raise ValueError('Node layout must not traverse symlinks')
    for path in [configuration/'node.json',configuration/'installation.json']:
        regular(path,[0])
    regular(database,[user.pw_uid])
    config=enrollment.read_json(configuration/'node.json');installed=enrollment.read_json(configuration/'installation.json')
    expected_format='bbsbox.install/v1' if release else 'bbsbox.development-install/v1'
    if config.get('database')!=str(database) or installed.get('format')!=expected_format:raise ValueError('Expected the installed node layout')
    stage=Path('/var/lib/bbsbox-maintenance');prepare_stage(stage)
    executable=Path('/opt/bbsbox/current/bbsbox-node')
    regular(executable.resolve(),[0])
    release_root=Path('/opt/bbsbox/releases') if release else Path('/opt/bbsbox/development')
    if not executable.resolve().is_relative_to(release_root):raise ValueError('Expected the installed application release')
    lock=os.open('/run/bbsbox-maintenance.lock',os.O_RDWR|os.O_CREAT|os.O_NOFOLLOW,0o600)
    resume=False
    try:
        metadata=os.fstat(lock)
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid!=0 or metadata.st_mode&0o077:raise ValueError('Unexpected maintenance lock ownership/mode')
        fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        if release:run(['/opt/bbsbox/maintenance/bbsbox-operator','release-control-installed'])
        active=subprocess.run(['/usr/bin/systemctl','is-active','bbsbox.service'],capture_output=True,timeout=20).stdout.strip()
        if active not in [b'active',b'inactive',b'failed']:raise ValueError('Wait for the service state transition before backup')
        was_active=active==b'active'
        try:
            if was_active:run(['/usr/bin/systemctl','stop','bbsbox.service'])
            game_backup=release and game_state_retained(config)
            profile='backup-game-node' if game_backup else 'backup'
            with checkpoint_socket(game_backup):
                completed=run([str(executable),profile,'--config',str(configuration/'node.json'),'--node',installed['node_id'],'--recipient',recipient,'--output',str(destination),'--staging',str(stage)],timeout=1200)
            if len(completed.stdout)>65536:raise ValueError('Unexpected maintenance response size')
            result=json.loads(completed.stdout)
            if result.get('format')!='bbsbox.node-backup/v1' or result.get('node')!=installed['node_id']:raise ValueError('Unexpected backup result')
            if game_backup and (result.get('scope')!='game-node' or result.get('game_checkpoints')!=1):raise ValueError('Incomplete game node backup')
        finally:
            repair_ownership(database,user.pw_uid,user.pw_gid)
            resume=was_active
        result.update(service_resumed=was_active,source_identity_preserved=True)
        return result
    finally:
        os.close(lock)
        if resume:run(['/usr/bin/systemctl','start','bbsbox.service'])


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--development',action='store_true',required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--recipient',required=True)
    args=parser.parse_args()
    print(json.dumps(backup(args.output,args.recipient),indent=2))


if __name__=='__main__':main()
