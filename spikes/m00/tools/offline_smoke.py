# SPDX-License-Identifier: AGPL-3.0-or-later
"""Run the actual node in an already isolated Linux network namespace and audit network syscalls."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import signal
import ssl
import subprocess
import tempfile
import time
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]


def audit_trace(text):
    """Require actual listener/accept evidence, and reject any outbound IP connect/send destination."""
    if 'bind(' not in text or 'listen(' not in text or not re.search(r'\baccept(?:4)?\(', text):
        raise ValueError('Incomplete trace: expected bind, listen and accepted traffic')
    if '<unfinished ...>' in text and re.search(r'\bconnect\(.*<unfinished', text):
        raise ValueError('Unfinished outbound connect cannot be treated as safe')
    # Match only syscall lines, not text embedded inside application payload strings.
    for line in text.splitlines():
        call = re.match(r'^\s*(?:\d+\s+)?(connect|sendto|sendmsg|sendmmsg)\(', line)
        if not call:
            continue
        if call[1] == 'connect':
            # This node is a server: even attempted/failed connect() is unexpected.
            raise ValueError('Unexpected outbound connect syscall')
        # sendto() on accepted TCP sockets with a NULL destination is ordinary serving.
        # Explicit destinations are unnecessary for this experiment and rejected.
        if call[1] == 'sendto' and not re.search(r', NULL, 0\)\s*=', line):
            raise ValueError('Unexpected explicit sendto destination')
        if call[1] in ('sendmsg', 'sendmmsg') and not re.search(r'msg_name=NULL', line):
            raise ValueError('Unexpected explicit message destination')
    return {'listener_observed': True, 'accepted_requests_observed': True, 'outbound_connects': 0, 'explicit_datagram_destinations': 0}


def check_namespace():
    if os.name != 'posix' or not Path('/proc/self/ns/net').exists():
        raise RuntimeError('This experiment needs Linux unshare, iproute2 and strace')
    if os.readlink('/proc/self/ns/net') == os.readlink('/proc/1/ns/net'):
        raise RuntimeError('Run inside a disposable network namespace, never on the host namespace')
    links = json.loads(subprocess.check_output(['ip', '-j', 'link', 'show']))
    if not links or any(link['ifname'] != 'lo' for link in links):
        raise RuntimeError('The test namespace must contain only loopback')
    if 'UP' not in links[0]['flags']:
        raise RuntimeError('Bring loopback up inside the namespace first')
    for family in ('-4', '-6'):
        routes = json.loads(subprocess.check_output(['ip', family, '-j', 'route', 'show', 'table', 'all']))
        if any(route.get('gateway') or route.get('dst') == 'default' for route in routes):
            raise RuntimeError('A default route or gateway is present in the isolated test')


def exercise(binary, state_root, host, port):
    with tempfile.TemporaryDirectory(prefix='offline-', dir=state_root) as work:
        state = Path(work).resolve()
        if not state.is_relative_to(state_root.resolve()):
            raise RuntimeError('State folder escaped the test root')
        tls = state/'tls'
        tls.mkdir()
        for name in ('ca.pem', 'server.pem', 'server-key.pem'):
            shutil.copyfile(state_root/'tls'/name, tls/name)
        url_host = f'[{host}]' if ':' in host else host
        origin = f'https://{url_host}:{port}'
        env = {'PATH': os.environ['PATH'], 'BBSBOX_STATE': str(state), 'BBSBOX_WEB': str(ROOT/'web/build'), 'BBSBOX_BIND': f'{url_host}:{port}'}
        trace = state/'network.trace'
        process = subprocess.Popen(['strace', '-f', '-s', '256', '-e', 'trace=network', '-o', str(trace), str(binary)], cwd=ROOT, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
        try:
            context = ssl.create_default_context(cafile=str(tls/'ca.pem'))
            # Explicitly disable proxy discovery: the only permitted request target is this loopback server.
            opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), urllib.request.HTTPSHandler(context=context))
            def get(path):
                return opener.open(origin+path, timeout=1)
            for _ in range(100):
                try:
                    with get('/api/node') as response:
                        info = json.load(response)
                    break
                except (OSError, urllib.error.URLError):
                    if process.poll() is not None:
                        raise RuntimeError('The traced node exited before readiness')
                    time.sleep(.1)
            else:
                raise RuntimeError('The traced node did not become ready')
            if info['messages'] != 0:
                raise RuntimeError('Expected fresh isolated test state')
            with get('/') as response:
                html = response.read().decode()
            if 'Good things' not in html:
                raise RuntimeError('The real static interface was not served')
            assets = re.findall(r'(?:href|src)="([^"#]+)"', html)
            asset_count = 0
            for asset in assets:
                if asset.startswith('data:') or asset.startswith('#'):
                    continue
                if asset.startswith(('https:', 'http:', '//')):
                    raise RuntimeError('External absolute asset in the static page')
                if asset.startswith(('_app/', './_app/', '/_app/')):
                    with get('/'+asset.removeprefix('./').lstrip('/')) as response:
                        if not response.read():
                            raise RuntimeError('Empty static asset')
                    asset_count += 1
            if asset_count == 0:
                raise RuntimeError('No real static assets were checked')
            body = json.dumps({'request_id': 'isolated-retry', 'label': 'Isolation fixture', 'text': 'Local signed message with no network route'}).encode()
            request = urllib.request.Request(origin+'/api/messages', data=body, headers={'Origin': origin, 'Content-Type': 'application/json'}, method='POST')
            with opener.open(request, timeout=3) as response:
                saved = json.load(response)
            with opener.open(request, timeout=3) as response:
                retried = json.load(response)
            with get('/api/messages') as response:
                messages = json.load(response)
            if not saved['verified'] or retried['id'] != saved['id'] or len(messages) != 1:
                raise RuntimeError('Isolated signing, retry or storage check failed')
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGINT)
            try:
                process.communicate(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.communicate()
                raise RuntimeError('The node did not shut down cleanly')
        if process.returncode not in (0, -signal.SIGINT):
            raise RuntimeError(f'The traced process exited unexpectedly: {process.returncode}')
        evidence_trace = state_root/('offline-ipv6.trace' if ':' in host else 'offline-ipv4.trace')
        shutil.copyfile(trace, evidence_trace)
        audit = audit_trace(trace.read_text(encoding='utf-8'))
        return dict(family='IPv6' if ':' in host else 'IPv4', static_assets_checked=asset_count, tls=True, signed_message=True, retry_deduplicated=True, **audit)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT/'node/target/debug/bbsbox-foundation')
    args = parser.parse_args()
    check_namespace()
    if not shutil.which('strace'):
        parser.error('Install strace in the disposable developer test environment')
    state = ROOT/'.state'
    results = [exercise(args.binary.resolve(), state, host, port) for host, port in (('127.0.0.1', 9543), ('::1', 9544))]
    report = dict(status='passed', environment='Linux network namespace; only loopback; no gateway/default route', cases=results, limitations='Local process/asset evidence only; no router, phone, DNS provisioning or production appliance claim')
    (state/'offline-evidence.json').write_text(json.dumps(report, indent=2)+'\n', encoding='utf-8')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
