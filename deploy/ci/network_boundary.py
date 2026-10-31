# SPDX-License-Identifier: AGPL-3.0-or-later
"""Exercise generated nftables/DNS rules only inside a fresh, gateway-free Linux namespace."""
import argparse
import ipaddress
import json
import os
from pathlib import Path
import socket
import socketserver
import struct
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT/'deploy'))
import network_profile


def run(*args, check=True):
    return subprocess.run(args, capture_output=True, check=check, timeout=10)


def namespace(pid, *args, check=True): return run('nsenter', '--target', str(pid), '--net', *args, check=check)


def query(host, name, version):
    question = b''.join(bytes([len(part)])+part.encode('ascii') for part in name.split('.'))+b'\0'+struct.pack('!HH', 1 if version == 4 else 28, 1)
    with socket.socket(socket.AF_INET6 if ':' in host else socket.AF_INET, socket.SOCK_DGRAM) as client:
        client.settimeout(2); client.sendto(struct.pack('!HHHHHH', 1234, 0x100, 1, 0, 0, 0)+question, (host, 53))
        response = client.recv(4096)
    ident, flags, _, count, _, _ = struct.unpack('!HHHHHH', response[:12])
    assert ident == 1234
    return response, flags & 15, count


def request(host, port=8443):
    with socket.create_connection((host, port), timeout=.6) as client:
        client.sendall(b'check'); return client.recv(16)


def serve(host, ready):
    class Echo(socketserver.BaseRequestHandler):
        def handle(self):
            self.request.settimeout(60)
            try:
                data = self.request.recv(64)
                if data: self.request.sendall(b'ok')
            except OSError: pass
    class Server(socketserver.ThreadingTCPServer):
        address_family = socket.AF_INET6 if ':' in host else socket.AF_INET
        allow_reuse_address = True
        daemon_threads = True
    with Server((host, 8443), Echo) as server:
        ready.write_text('ready', encoding='ascii'); server.serve_forever()


def exercise(output):
    if sys.platform != 'linux' or os.geteuid() != 0: raise RuntimeError('Use the isolated root CI runner')
    if os.readlink('/proc/self/ns/net') == os.readlink('/proc/1/ns/net'): raise RuntimeError('Refusing the host network namespace')
    links = json.loads(run('ip', '-j', 'link').stdout)
    tables = json.loads(run('nft', '-j', 'list', 'tables').stdout).get('nftables', [])
    if any(link['ifname'] != 'lo' for link in links) or any('table' in item for item in tables):
        raise RuntimeError('Require a fresh network namespace with only loopback and no firewall tables')
    output = output.resolve(); work = (ROOT/'work').resolve()
    if not output.is_relative_to(work) or output == work: raise RuntimeError('Evidence belongs beneath repository work/')
    output.mkdir(parents=True, exist_ok=False)
    processes = []; streams = []; capture = None
    def start(arguments, label):
        stream = (output/(label+'.log')).open('wb'); streams.append(stream)
        process = subprocess.Popen(arguments, stdout=stream, stderr=subprocess.STDOUT); processes.append(process)
        return process
    def start_server(pid, host, label):
        ready = output/(label+'.ready')
        prefix = ['nsenter', '--target', str(pid), '--net'] if pid else []
        process = start([*prefix, sys.executable, str(Path(__file__).resolve()), '--serve', host, '--output', str(ready)], label)
        for _ in range(100):
            if process.poll() is not None: raise RuntimeError(f'{label} failed')
            if ready.exists(): return
            time.sleep(.02)
        raise RuntimeError(f'{label} readiness timed out')
    try:
        nodes = {}
        values = [('client', '192.168.50', 'fd42:bb50'), ('peer', '192.168.51', 'fd42:bb51'), ('wan', '198.51.100', '2001:db8:bb52')]
        for label, v4, v6 in values:
            holder = start(['unshare', '--net', '--', 'sleep', '180'], label+'-namespace')
            for _ in range(100):
                if holder.poll() is not None: raise RuntimeError('Namespace holder failed')
                if os.readlink(f'/proc/{holder.pid}/ns/net') != os.readlink('/proc/self/ns/net'): break
                time.sleep(.01)
            else: raise RuntimeError('Private namespace not observed')
            nodes[label] = holder.pid
            local = 'bb-'+label; remote = 'r-'+label
            run('ip', 'link', 'add', local, 'type', 'veth', 'peer', 'name', remote)
            run('ip', 'link', 'set', remote, 'netns', str(holder.pid))
            namespace(holder.pid, 'ip', 'link', 'set', remote, 'name', 'eth0')
            for prefix, interface, suffix in [([], local, '1'), (['nsenter', '--target', str(holder.pid), '--net'], 'eth0', '2')]:
                run(*prefix, 'ip', 'addr', 'add', v4+'.'+suffix+'/24', 'dev', interface)
                run(*prefix, 'ip', '-6', 'addr', 'add', v6+'::'+suffix+'/64', 'dev', interface, 'nodad')
                run(*prefix, 'ip', 'link', 'set', interface, 'up')
            namespace(holder.pid, 'ip', 'link', 'set', 'lo', 'up')
            for address in [v4+'.2', v6+'::2']: start_server(holder.pid, address, label+('-v6' if ':' in address else '-v4'))
        run('ip', 'route', 'add', 'default', 'via', '198.51.100.2')
        run('ip', '-6', 'route', 'add', 'default', 'via', '2001:db8:bb52::2')
        namespace(nodes['client'], 'ip', 'route', 'add', 'default', 'via', '192.168.50.1')
        namespace(nodes['client'], 'ip', '-6', 'route', 'add', 'default', 'via', 'fd42:bb50::1')
        # Deliberately permissive kernel routing/default routes: the generated boundary
        # itself must still stop forwarding and pre-existing WAN connections.
        run('sysctl', '-w', 'net.ipv4.ip_forward=1', 'net.ipv6.conf.all.forwarding=1')
        for address in ['192.168.50.1', 'fd42:bb50::1']: start_server(None, address, 'node'+('-v6' if ':' in address else '-v4'))
        held = [socket.create_connection((address, 8443), timeout=2) for address in ['198.51.100.2', '2001:db8:bb52::2']]
        try:
            profile = json.loads((ROOT/'deploy/network.example.json').read_text(encoding='utf-8'))
            profile['interfaces'][0]['name'] = 'bb-client'
            profile['interfaces'].append(dict(name='bb-peer', role='peer', addresses=['192.168.51.1/24', 'fd42:bb51::1/64']))
            network_profile.prepare(profile, output/'profile')
            rules = output/'profile/bbsbox.nft'
            run('nft', '--check', '--file', str(rules)); run('nft', '--file', str(rules))
            capture = socket.socket(socket.AF_PACKET, socket.SOCK_RAW, socket.htons(3)); capture.bind(('bb-wan', 0)); capture.setblocking(False)
            dns = start(['dnsmasq', '--no-daemon', '--conf-file='+str(output/'profile/dnsmasq.conf'), '--pid-file=', '--log-facility=-', '--user=root'], 'dnsmasq')
            for _ in range(100):
                if dns.poll() is not None: raise RuntimeError('Local DNS failed')
                try:
                    query('127.0.0.1', profile['hostname'], 4); break
                except OSError: time.sleep(.02)
            else: raise RuntimeError('DNS readiness timed out')
            for version, own in [(4, '192.168.50.1'), (6, 'fd42:bb50::1')]:
                check = namespace(nodes['client'], sys.executable, str(Path(__file__).resolve()), '--client', own, '--hostname', profile['hostname'])
                assert json.loads(check.stdout)['passed']
                assert request('192.168.51.2' if version == 4 else 'fd42:bb51::2') == b'ok'
                for address in ['192.168.50.2', '198.51.100.2'] if version == 4 else ['fd42:bb50::2', '2001:db8:bb52::2']:
                    try: request(address); raise AssertionError('Unapproved outbound connection passed')
                    except OSError: pass
                forwarded = namespace(nodes['client'], sys.executable, str(Path(__file__).resolve()), '--blocked', '198.51.100.2' if version == 4 else '2001:db8:bb52::2')
                assert json.loads(forwarded.stdout)['blocked']
            for connection in held:
                connection.settimeout(.4)
                try:
                    connection.sendall(b'after-boundary'); connection.recv(16)
                    raise AssertionError('Existing WAN connection bypassed policy')
                except OSError: pass
            for host, port in [('8.8.8.8', 53), ('8.8.8.8', 123), ('2001:4860:4860::8888', 53), ('2001:4860:4860::8888', 123)]:
                with socket.socket(socket.AF_INET6 if ':' in host else socket.AF_INET, socket.SOCK_DGRAM) as client:
                    try: client.sendto(b'blocked probe', (host, port))
                    except OSError: pass
            leaked = 0
            while True:
                try: frame, address = capture.recvfrom(65536)
                except BlockingIOError: break
                if address[2] == socket.PACKET_OUTGOING and frame[12:14] in [b'\x08\x00', b'\x86\xdd']: leaked += 1
            assert leaked == 0, f'{leaked} IP packets escaped through the simulated WAN'
            counters = json.loads(run('nft', '-j', 'list', 'table', 'inet', 'bbsbox_boundary').stdout)
            (output/'counters.json').write_text(json.dumps(counters, indent=2)+'\n', encoding='utf-8')
            report = dict(status='passed', ipv4_and_ipv6=True, local_client_service_and_dns=True, explicit_peer_connections=True, client_initiation_blocked=True, wan_new_and_existing_connections_blocked=True, wan_dns_and_ntp_blocked=True, forwarding_blocked_with_default_routes=True, wan_outgoing_ip_packets=leaked, scope='Generated rules/DNS in simulated interfaces; host boot activation, physical router and phone gates remain separate')
            (output/'evidence.json').write_text(json.dumps(report, indent=2)+'\n', encoding='utf-8'); print(json.dumps(report, indent=2))
        finally:
            for connection in held: connection.close()
    finally:
        if capture: capture.close()
        for process in reversed(processes):
            if process.poll() is None: process.terminate()
        for process in reversed(processes):
            try: process.wait(timeout=3)
            except subprocess.TimeoutExpired: process.kill(); process.wait(timeout=3)
        for stream in streams: stream.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__); parser.add_argument('--output', type=Path, default=ROOT/'work/network-boundary'); parser.add_argument('--serve'); parser.add_argument('--client'); parser.add_argument('--hostname'); parser.add_argument('--blocked'); args = parser.parse_args()
    if args.serve: serve(args.serve, args.output)
    elif args.client:
        assert request(args.client) == b'ok'
        version = 6 if ':' in args.client else 4
        response, code, count = query(args.client, args.hostname, version)
        assert code == 0 and count and ipaddress.ip_address(args.client).packed in response
        _, code, count = query(args.client, 'unconfigured.example', version); assert code == 3 and count == 0
        print(json.dumps(dict(passed=True)))
    elif args.blocked:
        try: request(args.blocked); raise AssertionError('Forwarding passed')
        except OSError: print(json.dumps(dict(blocked=True)))
    else: exercise(args.output)


if __name__ == '__main__': main()
