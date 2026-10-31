# SPDX-License-Identifier: AGPL-3.0-or-later
"""Boot only the prepared developer OS tree in a private network and exercise its actual systemd unit."""
import argparse
import json
import os
from pathlib import Path
import signal
import ssl
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


def enter(pid,root,arguments,check=True):
    if not private_process(pid,root):raise RuntimeError('Refusing to enter anything except this private test OS')
    return subprocess.run(['nsenter','--target',str(pid),'--mount','--pid','--root','--wd','--',*arguments],capture_output=True,check=check,timeout=20)


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


def service_identity(pid,root):
    uid=int(enter(pid,root,['/usr/bin/id','-u','bbsbox']).stdout)
    if uid==0:raise RuntimeError('Service must be unprivileged')
    for process in Path('/proc').iterdir():
        if not process.name.isdecimal():continue
        try:
            if (process/'comm').read_text().strip()=='bbsbox-node' and same_root(int(process.name),root):
                cgroup=(process/'cgroup').read_text()
                status=(process/'status').read_text()
                actual=int(next(line for line in status.splitlines() if line.startswith('Uid:')).split()[1])
                if 'bbsbox.service' in cgroup and actual==uid:return int(process.name)
        except (OSError,StopIteration):continue
    raise RuntimeError('No unprivileged bbsbox process owned by the actual unit was observed')


def boot(root,output):
    if sys.platform!='linux' or os.geteuid()!=0:raise RuntimeError('Use a disposable Linux CI host as root')
    root=root.resolve();work=(ROOT/'work').resolve()
    if not root.is_relative_to(work) or root==work:raise RuntimeError('Test root must be beneath repository work/')
    installed=json.loads((root/'etc/bbsbox/installation.json').read_text())
    if installed['format']!='bbsbox.development-install/v1':raise RuntimeError('Expected the development install fixture')
    cases=[]
    for attempt in range(2):
        log=output.parent/f'debian-boot-{attempt}.log'
        with log.open('wb') as stream:
            process=subprocess.Popen(['systemd-nspawn','--settings=no','--register=no','--private-users=no','--directory',str(root),'--boot','--private-network','--console=pipe','--resolv-conf=off','--link-journal=no','--machine=bbsbox-ci'],stdout=stream,stderr=subprocess.STDOUT,start_new_session=True)
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
                    enter(pid,root,['/usr/bin/systemctl','stop','bbsbox.service'])
                    config_path=root/'etc/bbsbox/node.json';config=json.loads(config_path.read_text());config.update(bind='[::1]:8443',origin='https://[::1]:8443')
                    config_path.write_text(json.dumps(config)+'\n')
                    enter(pid,root,['/usr/bin/systemctl','start','bbsbox.service'])
                    pid,ipv6=wait_ready(process,root);assert ipv6['node_id']==info['node_id'];service_identity(pid,root)
                cases.append(dict(boot=attempt+1,actual_unit=True,unprivileged=True,https=True,restart_identity_preserved=True,isolated_loopback_only=True))
            except Exception:
                if pid and private_process(pid,root):
                    diagnostic=enter(pid,root,['/usr/bin/journalctl','--no-pager','-n','80','-u','bbsbox.service'],False)
                    print(diagnostic.stdout.decode(errors='replace'),file=sys.stderr)
                raise
            finally:
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
    report=dict(status='passed',target='Debian 13 amd64 userspace/systemd in systemd-nspawn',cases=cases,ipv4_and_ipv6=True,node_identity_preserved_after_os_reboot=True,scope='Install/service proof on the CI kernel; physical appliance/router/phone acceptance remains separate')
    output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--root',type=Path,required=True);parser.add_argument('--output',type=Path,default=ROOT/'work/debian-service-evidence.json');parser.add_argument('--probe',action='store_true');args=parser.parse_args()
    if args.probe:print(json.dumps(probe(args.root)))
    else:args.output.parent.mkdir(parents=True,exist_ok=True);boot(args.root,args.output)


if __name__=='__main__':main()
