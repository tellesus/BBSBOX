# SPDX-License-Identifier: AGPL-3.0-or-later
"""Exercise real HTTPS startup/restart; --isolated also audits both families in a Linux namespace."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import signal
import socket
import ssl
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('m00_network_audit', ROOT/'spikes/m00/tools/offline_smoke.py')
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)


def exercise(binary, state_root, host, isolated):
    family = socket.AF_INET6 if ':' in host else socket.AF_INET
    with socket.socket(family) as reserve:
        reserve.bind((host, 0))
        port = reserve.getsockname()[1]
    address = f'[{host}]' if ':' in host else host
    origin = f'https://{address}:{port}'
    other_host = '127.0.0.1' if family == socket.AF_INET6 else '::1'
    other_address = f'[{other_host}]' if ':' in other_host else other_host
    other_origin = f'https://{other_address}:{port}'
    with tempfile.TemporaryDirectory(prefix='runtime-', dir=state_root) as temporary:
        state = Path(temporary).resolve()
        if not state.is_relative_to(state_root):
            raise RuntimeError('Temporary state escaped the selected test directory')
        config = dict(format='bbsbox.node-config/v1', name='Runtime fixture <node>', bind=f'{address}:{port}', origin=origin, additional_origins=[f'https://localhost:{port}'], database=str(state/'node.sqlite'), certificate=str(state/'tls/server.pem'), certificate_key=str(state/'tls/server-key.pem'))
        config['additional_binds'] = [f'{other_address}:{port}']
        config['additional_origins'].append(other_origin)
        config_path = state/'node.json'
        config_path.write_text(json.dumps(config), encoding='utf-8')
        initialize=[str(binary),'--config',str(config_path),'--initialize']
        init_trace=state/'initialization.trace'
        if isolated:
            initialize=['strace','-f','-s','256','-e','trace=network','-o',str(init_trace),*initialize]
        initialized=json.loads(subprocess.check_output(initialize,timeout=15,creationflags=subprocess.CREATE_NO_WINDOW if os.name=='nt' else 0))
        assert initialized['format']=='bbsbox.node-init/v1'
        initial_audit=[]
        if isolated:
            text=init_trace.read_text(encoding='utf-8')
            initial_audit.append(audit.audit_trace(text,require_listener=False))
            (state_root/f'runtime-{"ipv6" if family==socket.AF_INET6 else "ipv4"}-initialization.trace').write_text(text,encoding='utf-8')
        subprocess.run([sys.executable, str(ROOT/'spikes/m00/tools/init_tls.py'), '--state', str(state)], check=True, capture_output=True)
        context = ssl.create_default_context(cafile=str(state/'tls/ca.pem'))
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), urllib.request.HTTPSHandler(context=context))
        def request(path, method='GET', headers=None):
            return opener.open(urllib.request.Request(origin+path, method=method, headers=headers or {}), timeout=2)
        identity = initialized['node_id']
        traces = []
        for attempt in range(2):
            command = [str(binary), '--config', str(config_path)]
            trace = state/f'network-{attempt}.trace'
            if isolated:
                command = ['strace', '-f', '-s', '256', '-e', 'trace=network', '-o', str(trace), *command]
            process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=os.name == 'posix', creationflags=subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0)
            try:
                for _ in range(100):
                    if process.poll() is not None:
                        output, errors = process.communicate()
                        raise RuntimeError(f'Runtime exited before readiness ({process.returncode}): {errors.decode(errors="replace")[:1500]}')
                    try:
                        with request('/api/v1/node') as response:
                            info = json.load(response)
                        break
                    except (OSError, urllib.error.URLError):
                        time.sleep(.1)
                else:
                    raise RuntimeError('Runtime did not become ready')
                assert set(info) == {'format', 'name', 'node_id', 'version', 'status'}
                assert info['status'] == 'ready' and re.fullmatch(r'node:[a-f0-9]{64}', info['node_id'])
                if identity is not None:
                    assert info['node_id'] == identity, 'Node identity changed after restart'
                identity = info['node_id']
                with request('/api/v1/node', headers={'Host': f'localhost:{port}'}) as response:
                    assert json.load(response)['node_id'] == identity
                with opener.open(other_origin+'/api/v1/node', timeout=2) as response:
                    assert json.load(response)['node_id'] == identity
                with request('/') as response:
                    html = response.read().decode()
                    assert 'default-src' in response.headers['Content-Security-Policy']
                assets = re.findall(r'<(?:link|script)\b[^>]*(?:href|src)="([^"#]+)"', html)
                assert set(assets) == {'/assets/status.css', '/assets/status.js'}
                for path in assets:
                    with request(path) as response:
                        assert response.read()
                for method, headers, expected in [('GET', {'Host': 'foreign.invalid'}, 421), ('POST', {}, 405)]:
                    try:
                        request('/api/v1/node', method, headers)
                        raise AssertionError('Forbidden request was accepted')
                    except urllib.error.HTTPError as error:
                        assert error.code == expected
                # A new client without the local CA must fail certificate verification.
                untrusted = urllib.request.build_opener(urllib.request.ProxyHandler({}), urllib.request.HTTPSHandler(context=ssl.create_default_context()))
                try:
                    untrusted.open(origin+'/api/v1/node', timeout=2)
                    raise AssertionError('Self-signed local CA was implicitly trusted')
                except urllib.error.URLError as error:
                    assert isinstance(error.reason, ssl.SSLCertVerificationError)
            finally:
                if process.poll() is None:
                    if os.name == 'posix':
                        os.killpg(process.pid, signal.SIGTERM)
                    else:
                        process.terminate()  # Windows process-loss case; Linux tests service SIGTERM.
                try:
                    _, errors = process.communicate(timeout=15)
                except subprocess.TimeoutExpired:
                    if os.name == 'posix':
                        os.killpg(process.pid, signal.SIGKILL)
                    else:
                        process.kill()
                    process.communicate()
                    raise RuntimeError('Runtime did not stop within its drain timeout')
                if os.name == 'posix' and process.returncode not in (0, -signal.SIGTERM, 128+signal.SIGTERM):
                    raise RuntimeError(f'Unexpected runtime stop: {errors.decode(errors="replace")[:1500]}')
            if isolated:
                text = trace.read_text(encoding='utf-8')
                traces.append(audit.audit_trace(text))
                (state_root/f'runtime-{"ipv6" if family == socket.AF_INET6 else "ipv4"}-{attempt}.trace').write_text(text, encoding='utf-8')
        return dict(family='IPv6' if family == socket.AF_INET6 else 'IPv4', simultaneous_dual_stack=True, tls=True, untrusted_ca_rejected=True, local_assets=2, identity_preserved_after_restart=True, configured_alias_accepted=True, unexpected_authority_rejected=True, mutation_route_absent=True, initialization_audit=initial_audit, network_audit=traces)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT/('target/debug/bbsbox-node.exe' if os.name == 'nt' else 'target/debug/bbsbox-node'))
    parser.add_argument('--state', type=Path, default=ROOT/'work/runtime-evidence')
    parser.add_argument('--isolated', action='store_true')
    args = parser.parse_args()
    state = args.state.resolve()
    state.mkdir(parents=True, exist_ok=True)
    if args.isolated:
        audit.check_namespace()
    cases = [exercise(args.binary.resolve(), state, host, args.isolated) for host in ['127.0.0.1', '::1']]
    report = dict(status='passed', isolated=args.isolated, cases=cases, scope='Real process/TLS/restart evidence; Debian installation, router/DNS enrollment and phone tests remain separate')
    (state/'runtime-evidence.json').write_text(json.dumps(report, indent=2)+'\n', encoding='utf-8')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
