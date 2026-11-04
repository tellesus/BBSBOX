# SPDX-License-Identifier: AGPL-3.0-or-later
"""Boot the prepared Debian network profile with an actual local client link and no WAN."""
import argparse
import hashlib
import http.client
import json
import os
from pathlib import Path
import signal
import socket
import ssl
import subprocess
import time

import boot_service as service
from network_boundary import query
from link_capture import Capture

ROOT = Path(__file__).resolve().parents[2]


def run(*args): return subprocess.run(args, check=True, capture_output=True, timeout=10)


def request(root, hostname, address, path, cookie=None):
    context = ssl.create_default_context(cafile=str(root/'etc/bbsbox/tls/ca.pem'))
    with socket.create_connection((address,8443),timeout=2) as plain:
        with context.wrap_socket(plain,server_hostname=hostname) as secure:
            fingerprint=hashlib.sha256(secure.getpeercert(binary_form=True)).hexdigest()
            cookie_header = f'Cookie: {cookie}\r\n' if cookie else ''
            secure.sendall(f'GET {path} HTTP/1.1\r\nHost: {hostname}:8443\r\n{cookie_header}Connection: close\r\n\r\n'.encode())
            response = http.client.HTTPResponse(secure); response.begin()
            return response.status, response.read(8192), response.headers, fingerprint


def fetch(root, hostname, address, with_fingerprint=False):
    status, body, _, fingerprint = request(root,hostname,address,'/api/v1/node')
    assert status==200
    info = json.loads(body)
    return (info,fingerprint) if with_fingerprint else info


def exercise(root, output):
    root = root.resolve(); work = (ROOT/'work').resolve()
    if os.geteuid()!=0 or os.readlink('/proc/self/ns/net')==os.readlink('/proc/1/ns/net') or any(link['ifname']!='lo' for link in json.loads(run('ip','-j','link').stdout)):
        raise RuntimeError('Require a fresh private Linux network with only loopback')
    if root==work or not root.is_relative_to(work): raise RuntimeError('Boot only the prepared repository work/ tree')
    installed = json.loads((root/'etc/bbsbox/installation.json').read_text())
    profile = json.loads((root/'etc/bbsbox/network/profile.json').read_text())
    game_host = installed['hostname'].replace('bbsbox-', 'bbsbox-game-', 1) if profile.get('prepare_game_host') else None
    game_enabled = bool(json.loads((root/'etc/bbsbox/node.json').read_text()).get('game_origin'))
    if installed['format'] not in ('bbsbox.development-install/v1','bbsbox.install/v1') or profile['interfaces']!=[dict(name='bbs-client',role='client',addresses=['192.168.50.1/24','fd42:bb50::1/64'])]:
        raise RuntimeError('Use only the known local client fixture, without peers')
    output.mkdir(parents=True,exist_ok=False); cases=[]
    for attempt in range(2):
        run('ip','link','add','bbs-client','type','veth','peer','name','bbs-visitor')
        run('ip','addr','add','192.168.50.2/24','dev','bbs-visitor')
        run('ip','-6','addr','add','fd42:bb50::2/64','dev','bbs-visitor','nodad')
        run('ip','link','set','bbs-visitor','up')
        with (output/f'network-boot-{attempt}.log').open('wb') as stream, Capture('bbs-visitor',output/f'boot-{attempt}.pcap') as capture:
            process = subprocess.Popen(['systemd-nspawn','--settings=no','--register=no','--private-users=no','--directory',str(root),'--boot','--private-network','--console=read-only','--resolv-conf=off','--link-journal=no',f'--machine=bbsbox-net-{attempt}-{os.getpid()}','systemd.log_target=console'],stdout=stream,stderr=subprocess.STDOUT,start_new_session=True)
            pid=None; pid_fd=None
            try:
                until=time.monotonic()+60; attached=False
                while time.monotonic()<until:
                    if process.poll() is not None: raise RuntimeError('Prepared network OS stopped')
                    pid=service.leader(root)
                    if pid and not attached:
                        # netlink moves only our fixture interface into the validated guest.
                        # nspawn's preboot device lookup uses the outer /sys mount, which
                        # need not expose devices created in this parent network namespace.
                        run('ip','link','set','bbs-client','netns',str(pid)); attached=True
                    try:
                        if pid and service.enter(pid,root,['/usr/bin/systemctl','is-active','bbsbox.service'],False).stdout.strip()==b'active':
                            info=fetch(root,installed['hostname'],'192.168.50.1'); break
                    except (OSError,subprocess.SubprocessError): pass
                    time.sleep(.2)
                else: raise RuntimeError('Prepared network service readiness timed out')
                pid_fd=os.pidfd_open(pid)
                assert info['node_id']==installed['node_id']; service.service_identity(pid,root)
                if attempt==0:
                    _,previous=fetch(root,installed['hostname'],'192.168.50.1',True)
                    renewal=json.loads(service.enter(pid,root,['/usr/bin/python3','/src/deploy/renew_tls.py','--development']).stdout)
                    assert renewal['ca_sha256']==installed['ca_sha256'] and not renewal['ca_replaced']
                    assert fetch(root,installed['hostname'],'192.168.50.1',True)[1]==previous
                    service.enter(pid,root,['/usr/bin/systemctl','restart','bbsbox.service'])
                    for _ in range(100):
                        try:
                            current,fingerprint=fetch(root,installed['hostname'],'192.168.50.1',True);break
                        except OSError:time.sleep(.1)
                    else:raise RuntimeError('Runtime did not restart with renewed TLS')
                    assert current['node_id']==installed['node_id'] and fingerprint==renewal['leaf_sha256'] and fingerprint!=previous
                    (output/'renewal.json').write_text(json.dumps(renewal,indent=2)+'\n')
                for version,address in [(4,'192.168.50.1'),(6,'fd42:bb50::1')]:
                    assert fetch(root,installed['hostname'],address)['node_id']==installed['node_id']
                    response,code,count=query(address,installed['hostname'],version)
                    assert code==0 and count
                    if game_host:
                        _,code,count=query(address,game_host,version)
                        assert code==0 and count
                        status,body,headers,leaf = request(root,game_host,address,'/')
                        assert leaf==fetch(root,installed['hostname'],address,True)[1]
                        if game_enabled:
                            assert status==200 and b'Dungeon demo' in body and 'Set-Cookie' not in headers
                            assert request(root,game_host,address,'/account/')[0]==404
                            assert request(root,game_host,address,'/','__Host-bbsbox_session=fixture')[0]==403
                        else:
                            assert status==421
                    _,code,count=query(address,'unconfigured.example',version); assert code==3 and count==0
                for unit in ['bbsbox-boundary.service','bbsbox-dns.service','systemd-networkd.service']:
                    assert service.enter(pid,root,['/usr/bin/systemctl','is-active',unit]).stdout.strip()==b'active'
                service.service_identity(pid,root,'bbsbox-dns','dnsmasq','bbsbox-dns.service')
                for family in ['-4','-6']:
                    routes=json.loads(service.enter(pid,root,['/usr/sbin/ip',family,'-j','route','show','table','all']).stdout)
                    assert not any(route.get('gateway') or route.get('dst')=='default' for route in routes)
                for key in ['net.ipv4.ip_forward','net.ipv6.conf.all.forwarding']:
                    assert service.enter(pid,root,['/usr/bin/cat','/proc/sys/'+key.replace('.','/')]).stdout.strip()==b'0'
                for unit in ['systemd-timesyncd.service','apt-daily.service','systemd-resolved.service','dnsmasq.service','nftables.service','systemd-network-generator.service']:
                    assert service.enter(pid,root,['/usr/bin/systemctl','is-enabled',unit],False).stdout.strip()==b'masked'
                counters=service.enter(pid,root,['/usr/sbin/nft','-j','list','table','inet','bbsbox_boundary']).stdout
                (output/f'boundary-{attempt}.json').write_bytes(counters)
                diagnostic=service.enter(pid,root,['/usr/bin/python3','/src/deploy/diagnose.py'],False)
                (output/f'diagnostic-{attempt}.json').write_bytes(diagnostic.stdout)
                assert diagnostic.returncode==0,diagnostic.stdout.decode(errors='replace')
                cases.append(dict(boot=attempt+1,local_client_https_ipv4_ipv6=True,dns_local_only=True,actual_networkd_boundary_dns_units=True,node_identity_and_ca_preserved=True,no_default_route=True,forwarding_disabled=True,online_services_masked=True,no_peers=True))
            except Exception:
                pid=pid or service.leader(root)
                if pid:
                    try: print(service.enter(pid,root,['/usr/bin/journalctl','--no-pager','-n','120'],False).stdout.decode(errors='replace'))
                    except (OSError,subprocess.SubprocessError): pass
                raise
            finally:
                pid=pid or service.leader(root)
                if pid_fd is None and pid and service.private_process(pid,root):
                    try: pid_fd=os.pidfd_open(pid)
                    except ProcessLookupError: pass
                if process.poll() is None: process.terminate()
                try: process.wait(timeout=25)
                except subprocess.TimeoutExpired:
                    if pid_fd is not None:
                        try: signal.pidfd_send_signal(pid_fd,signal.SIGKILL)
                        except ProcessLookupError: pass
                    if process.poll() is None: os.killpg(process.pid,signal.SIGKILL)
                    process.wait(timeout=5); raise RuntimeError('Prepared network OS failed orderly shutdown')
                finally:
                    if pid_fd is not None: os.close(pid_fd)
                # These names were created only in the validated private parent namespace.
                if subprocess.run(['ip','link','show','dev','bbs-visitor'],capture_output=True).returncode==0:
                    run('ip','link','delete','bbs-visitor')
        if process.returncode: raise RuntimeError('Prepared network OS shutdown failed')
        assert capture.report['status']=='passed',capture.report
        cases[-1]['boot_packet_capture']=capture.report
    report=dict(status='passed',cases=cases,local_leaf_renewal_and_restart_verified=True,prepared_game_alias_verified=bool(game_host),isolated_game_origin_served=game_enabled,complete_boot_link_capture=True,scope='Actual Debian userspace/systemd with simulated local link; physical router/radio/phone acceptance remains separate')
    (output/'evidence.json').write_text(json.dumps(report,indent=2)+'\n'); print(json.dumps(report,indent=2))


def main():
    parser=argparse.ArgumentParser(description=__doc__); parser.add_argument('--root',type=Path,required=True); parser.add_argument('--output',type=Path,default=ROOT/'work/debian-network'); args=parser.parse_args(); exercise(args.root,args.output)


if __name__=='__main__': main()
