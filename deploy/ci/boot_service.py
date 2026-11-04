# SPDX-License-Identifier: AGPL-3.0-or-later
"""Boot only the prepared developer OS tree in a private network and exercise its actual systemd unit."""
import argparse
from contextlib import closing
import json
import os
from pathlib import Path
import signal
import ssl
import sqlite3
import stat
import subprocess
import sys
import time
import urllib.request
from urllib.parse import urlsplit

ROOT=Path(__file__).resolve().parents[2]


def same_root(pid,root):
    try:
        current=os.stat(f'/proc/{pid}/root');expected=root.stat()
        return (current.st_dev,current.st_ino)==(expected.st_dev,expected.st_ino)
    except OSError:return False


def private_process(pid,root):
    try:return pid>1 and same_root(pid,root) and all(os.readlink(f'/proc/{pid}/ns/{kind}')!=os.readlink(f'/proc/self/ns/{kind}') for kind in ['pid','net'])
    except OSError:return False


def leader(root):
    for process in Path('/proc').iterdir():
        if not process.name.isdecimal():continue
        pid=int(process.name)
        try:
            nested=next(line for line in (process/'status').read_text().splitlines() if line.startswith('NSpid:')).split()[1:]
            if nested[-1]=='1' and private_process(pid,root):return pid
        except (OSError,StopIteration):continue
    return None


def enter(pid,root,arguments,check=True,timeout=20):
    if not private_process(pid,root):raise RuntimeError('Refusing to enter anything except this private test OS')
    try:return subprocess.run(['nsenter','--target',str(pid),'--mount','--pid','--net','--root','--wd','--',*arguments],capture_output=True,check=check,timeout=timeout)
    except subprocess.CalledProcessError as error:
        print(error.stderr[-16384:].decode(errors='replace'),file=sys.stderr)
        raise


def maintenance(pid,root,node,signed=False):
    binary='/opt/bbsbox/current/bbsbox-node'
    backup_command=['/usr/bin/python3','/src/deploy/backup_node.py'] if signed else ['/usr/bin/python3','/src/deploy/backup_development.py','--development']
    recipient=json.loads(enter(pid,root,[binary,'backup-keygen','--output','/root/bbsbox-ci-backup.key']).stdout)['recipient']
    original_ca=(root/'etc/bbsbox/tls/ca-key.pem').read_bytes()
    result=json.loads(enter(pid,root,[*backup_command,'--output','/root/bbsbox-ci-backup.age','--recipient',recipient],timeout=120).stdout)
    assert result['node']==node and result['service_resumed'] and result['scope']=='foundation-node'
    assert enter(pid,root,['/usr/bin/systemctl','is-active','bbsbox.service']).stdout.strip()==b'active'
    service_identity(pid,root)
    restored=json.loads(enter(pid,root,[binary,'restore','--archive','/root/bbsbox-ci-backup.age','--node',node,'--identity-file','/root/bbsbox-ci-backup.key','--output','/root/bbsbox-ci-restored','--staging','/var/lib/bbsbox-maintenance'],timeout=120).stdout)
    assert restored['state']=='recovery-required' and restored['node']==node
    target=root/'root/bbsbox-ci-restored'
    assert (target/'data/node.sqlite.recovery-required').is_file()
    assert not (target/'.restore-incomplete').exists()
    assert (target/'configuration/tls/ca-key.pem').read_bytes()==original_ca
    assert (root/'etc/bbsbox/tls/ca-key.pem').read_bytes()==original_ca
    recovery=json.loads((target/'recovery.json').read_text())
    assert recovery['state']=='recovery-required' and recovery['post_snapshot_writes']=='not-reconciled'
    # Obtain IDs from this guest, then inspect only known files in its verified OS tree.
    uid=int(enter(pid,root,['/usr/bin/id','-u','bbsbox']).stdout)
    gid=int(enter(pid,root,['/usr/bin/id','-g','bbsbox']).stdout)
    for suffix in ['', '-wal', '-shm', '-journal', '.writer.lock']:
        try:metadata=Path(str(root/'var/lib/bbsbox/node.sqlite')+suffix).lstat()
        except FileNotFoundError:
            if suffix=='':raise
            continue
        assert stat.S_ISREG(metadata.st_mode) and metadata.st_uid==uid and metadata.st_gid==gid and stat.S_IMODE(metadata.st_mode)==0o600
    failed=enter(pid,root,[*backup_command,'--output','/root/bbsbox-ci-invalid.age','--recipient','age1invalid'],check=False,timeout=120)
    assert failed.returncode!=0 and not (root/'root/bbsbox-ci-invalid.age').exists()
    assert enter(pid,root,['/usr/bin/systemctl','is-active','bbsbox.service']).stdout.strip()==b'active'
    service_identity(pid,root)
    enter(pid,root,['/usr/bin/systemctl','stop','bbsbox.service'])
    inactive=json.loads(enter(pid,root,[*backup_command,'--output','/root/bbsbox-ci-inactive.age','--recipient',recipient],timeout=120).stdout)
    assert inactive['service_resumed'] is False and inactive['node']==node
    assert enter(pid,root,['/usr/bin/systemctl','is-active','bbsbox.service'],False).stdout.strip()==b'inactive'
    enter(pid,root,['/usr/bin/systemctl','start','bbsbox.service'])
    return dict(cold_backup=True,actual_root_ca_preserved=True,source_ownership_private=True,service_resumed_unprivileged=True,failed_backup_resumes_source=True,inactive_source_stays_inactive=True,isolated_restore=True,recovery_required=True)


def game_backup(pid, root, node):
    binary = '/opt/bbsbox/current/bbsbox-node'
    identity = '/root/bbsbox-ci-backup.key'
    archive = '/root/bbsbox-ci-game-backup.age'
    restored = '/root/bbsbox-ci-game-restore'
    recipient = json.loads(enter(pid,root,[binary,'backup-public','--identity-file',identity]).stdout)['recipient']
    enter(pid,root,['/usr/bin/systemctl','reset-failed','bbsbox.service'])
    saved = json.loads(enter(pid,root,['/usr/bin/python3','/src/deploy/backup_node.py','--output',archive,'--recipient',recipient],timeout=120).stdout)
    assert saved['scope']=='game-node' and saved['game_checkpoints']==1 and saved['service_resumed']
    opened = json.loads(enter(pid,root,[binary,'restore-game-node','--archive',archive,'--node',node,'--identity-file',identity,'--output',restored,'--staging','/var/lib/bbsbox-maintenance'],timeout=120).stdout)
    assert opened['inventory_sha256']==saved['inventory_sha256'] and opened['state']=='recovery-required'
    save = root/restored.lstrip('/')/'games/dungeon-demo/state.sqlite'
    with closing(sqlite3.connect(save.as_uri()+'?mode=ro',uri=True)) as database:
        assert database.execute('SELECT chamber FROM dungeon').fetchone()==(1,)
        assert database.execute('SELECT count(*) FROM players').fetchone()==(2,)
        assert database.execute('SELECT count(*) FROM receipts').fetchone()==(1,)
    return dict(complete_checkpoint=True, saved_turn_restored=True, recovery_fence_retained=True)


def probe(root):
    config=json.loads((root/'etc/bbsbox/node.json').read_text())
    origin=urlsplit(config['origin'])
    if origin.scheme!='https' or origin.hostname not in ['localhost','127.0.0.1','::1'] or origin.port!=8443:raise RuntimeError('CI probe accepts only the fixture loopback HTTPS address')
    ca=root/'etc/bbsbox/tls/ca.pem'
    opener=urllib.request.build_opener(urllib.request.ProxyHandler({}),urllib.request.HTTPSHandler(context=ssl.create_default_context(cafile=str(ca))))
    with opener.open(config['origin']+'/api/v1/node',timeout=2) as response:return json.load(response)


def wait_ready(process,root,deadline=60):
    until=time.monotonic()+deadline
    while time.monotonic()<until:
        if process.poll() is not None:raise RuntimeError('Test OS stopped before the service became ready')
        pid=leader(root)
        if pid:
            active=enter(pid,root,['/usr/bin/systemctl','is-active','bbsbox.service'],False)
            if active.stdout.strip()==b'active':
                try:
                    info=subprocess.check_output(['nsenter','--target',str(pid),'--net',sys.executable,str(Path(__file__).resolve()),'--root',str(root),'--probe'],stderr=subprocess.PIPE,timeout=5)
                    return pid,json.loads(info)
                except (subprocess.SubprocessError,json.JSONDecodeError):pass
        time.sleep(.2)
    raise RuntimeError('Test service readiness timed out')


def network(pid):
    def ip(*args):return json.loads(subprocess.check_output(['nsenter','--target',str(pid),'--net','ip',*args]))
    links=ip('-j','link','show')
    if not links or any(link['ifname']!='lo' for link in links):raise RuntimeError('Test OS has a non-loopback interface')
    for family in ['-4','-6']:
        if any(route.get('gateway') or route.get('dst')=='default' for route in ip(family,'-j','route','show','table','all')):raise RuntimeError('Test OS has an external route')


def service_identity(pid,root,user='bbsbox',process_name='bbsbox-node',unit='bbsbox.service'):
    uid=int(enter(pid,root,['/usr/bin/id','-u',user]).stdout)
    if uid==0:raise RuntimeError('Service must be unprivileged')
    observed=[]
    for process in Path('/proc').iterdir():
        if not process.name.isdecimal():continue
        try:
            if (process/'comm').read_text().strip()==process_name:
                root_matches=same_root(int(process.name),root)
                cgroup=(process/'cgroup').read_text()
                status=(process/'status').read_text()
                actual=[int(value) for value in next(line for line in status.splitlines() if line.startswith('Uid:')).split()[1:]]
                if len(observed)<8:observed.append(dict(pid=int(process.name),uids=actual,root_matches=root_matches,in_unit=unit in cgroup))
                if root_matches and unit in cgroup and actual==[uid]*4:return int(process.name)
        except (OSError,StopIteration):continue
    raise RuntimeError(f'Expected {process_name} as {user} (uid {uid}) in {unit}; observed {observed}')


def boot(root,output,signed=False,maintenance_check=None):
    if sys.platform!='linux' or os.geteuid()!=0:raise RuntimeError('Use a disposable Linux CI host as root')
    root=root.resolve();work=(ROOT/'work').resolve()
    if not root.is_relative_to(work) or root==work:raise RuntimeError('Test root must be beneath repository work/')
    installed=json.loads((root/'etc/bbsbox/installation.json').read_text())
    if installed['format']!=('bbsbox.install/v1' if signed else 'bbsbox.development-install/v1'):raise RuntimeError('Unexpected installation fixture format')
    if signed:
        worker=root/'opt/bbsbox/releases'/installed['release_id']/'bbsbox-demo-worker'
        metadata=worker.lstat()
        assert stat.S_ISREG(metadata.st_mode) and stat.S_IMODE(metadata.st_mode)==0o755
        assert metadata.st_uid==0 and metadata.st_gid==0
    probe_game_worker = signed and maintenance_check is None
    if probe_game_worker:
        files = {
            'etc/systemd/system/bbsbox-game-worker.socket': ROOT/'deploy/systemd/bbsbox-game-worker.socket',
            'etc/systemd/system/bbsbox-game-worker@.service': ROOT/'deploy/systemd/bbsbox-game-worker@.service',
            'usr/local/lib/bbsbox-game-worker-probe.py': ROOT/'deploy/ci/game_worker_case.py',
        }
        for name, source in files.items():
            target = root/name
            target.parent.mkdir(parents=True, exist_ok=True)
            with target.open('xb') as file:
                file.write(source.read_bytes())
            target.chmod(0o644)
    cases=[]
    for attempt in range(2):
        log=output.parent/f'debian-boot-{attempt}.log'
        with log.open('wb') as stream:
            process=subprocess.Popen(['systemd-nspawn','--settings=no','--register=no','--private-users=no','--directory',str(root),'--boot','--private-network','--console=read-only','--resolv-conf=off','--link-journal=no',f'--machine=bbsbox-ci-{attempt}-{os.getpid()}','systemd.log_target=console','systemd.show_status=yes'],stdout=stream,stderr=subprocess.STDOUT,start_new_session=True)
            pid=None;pid_fd=None
            try:
                pid,info=wait_ready(process,root)
                pid_fd=os.pidfd_open(pid)
                network(pid)
                assert info['node_id']==installed['node_id']
                first_pid=service_identity(pid,root)
                enter(pid,root,['/usr/bin/systemctl','restart','bbsbox.service'])
                pid,restarted=wait_ready(process,root)
                assert restarted['node_id']==info['node_id'] and service_identity(pid,root)!=first_pid
                if attempt==0:
                    if maintenance_check is not None:
                        maintenance_result=maintenance_check(pid,root,info['node_id'])
                    else:
                        maintenance_result=maintenance(pid,root,info['node_id'],signed)
                        # Keep the production crash-loop limit; reset only this fixture's counter.
                        assert int(enter(pid,root,['/usr/bin/systemctl','show','bbsbox.service','-p','StartLimitBurst','--value']).stdout)==5
                        enter(pid,root,['/usr/bin/systemctl','reset-failed','bbsbox.service'])
                        if signed:
                            import release_boot_cases
                            maintenance_result['release_control']=release_boot_cases.exercise(pid,root)
                            enter(pid,root,['/usr/sbin/useradd','--system','--user-group','--home-dir','/var/lib/bbsbox-game','--shell','/usr/sbin/nologin','bbsbox-game'])
                            socket_unit = 'bbsbox-game-worker.socket'
                            enter(pid,root,['/usr/bin/systemctl','start',socket_unit])
                            try:
                                probe = enter(pid,root,['/usr/bin/python3','/usr/local/lib/bbsbox-game-worker-probe.py','probe'],timeout=35)
                                maintenance_result['isolated_game_worker']=json.loads(probe.stdout)
                                maintenance_result['game_node_backup']=game_backup(pid,root,info['node_id'])
                            finally:
                                enter(pid,root,['/usr/bin/systemctl','stop',socket_unit])
                    enter(pid,root,['/usr/bin/systemctl','stop','bbsbox.service'])
                    config_path=root/'etc/bbsbox/node.json';config=json.loads(config_path.read_text());config.update(bind='[::1]:8443',origin='https://[::1]:8443')
                    config_path.write_text(json.dumps(config)+'\n')
                    enter(pid,root,['/usr/bin/systemctl','start','bbsbox.service'])
                    pid,ipv6=wait_ready(process,root);assert ipv6['node_id']==info['node_id'];service_identity(pid,root)
                cases.append(dict(boot=attempt+1,actual_unit=True,unprivileged=True,https=True,restart_identity_preserved=True,isolated_loopback_only=True,maintenance=maintenance_result if attempt==0 else None))
            except Exception:
                pid=pid or leader(root)
                if pid and private_process(pid,root):
                    try:
                        diagnostic=enter(pid,root,['/usr/bin/journalctl','--no-pager','-n','80','-u','bbsbox.service'],False)
                        print(diagnostic.stdout.decode(errors='replace'),file=sys.stderr)
                    except (OSError,subprocess.SubprocessError) as error:print(f'Guest journal unavailable: {error}',file=sys.stderr)
                raise
            finally:
                pid=pid or leader(root)
                if pid_fd is None and pid and private_process(pid,root):
                    try:pid_fd=os.pidfd_open(pid)
                    except ProcessLookupError:pass
                # Terminate only our unreaped nspawn child. Its documented --boot behavior
                # requests orderly guest shutdown; never issue a host poweroff command.
                if process.poll() is None:process.terminate()
                try:process.wait(timeout=25)
                except subprocess.TimeoutExpired:
                    if pid_fd is not None:
                        try:signal.pidfd_send_signal(pid_fd,signal.SIGKILL)
                        except ProcessLookupError:pass
                    if process.poll() is None:
                        try:os.killpg(process.pid,signal.SIGKILL)
                        except ProcessLookupError:pass
                    process.wait(timeout=5)
                    raise RuntimeError('Test OS failed orderly shutdown')
                finally:
                    if pid_fd is not None:os.close(pid_fd)
        if process.returncode!=0:raise RuntimeError(f'Test OS shutdown failed; inspect {log}')
    report=dict(status='passed',target='Debian 13 amd64 userspace/systemd in systemd-nspawn',cases=cases,ipv4_and_ipv6=True,node_identity_preserved_after_os_reboot=True,signed_release=signed,scope='Install/service proof on the CI kernel; physical appliance/router/phone acceptance remains separate')
    output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--root',type=Path,required=True);parser.add_argument('--output',type=Path,default=ROOT/'work/debian-service-evidence.json');parser.add_argument('--probe',action='store_true');parser.add_argument('--signed',action='store_true');args=parser.parse_args()
    if args.probe:print(json.dumps(probe(args.root)))
    else:args.output.parent.mkdir(parents=True,exist_ok=True);boot(args.root,args.output,args.signed)


if __name__=='__main__':main()
