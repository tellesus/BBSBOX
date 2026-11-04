# SPDX-License-Identifier: AGPL-3.0-or-later
"""Exercise real HTTPS startup/restart; --isolated also audits both families in a Linux namespace."""
import argparse
import http.cookiejar
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
import urllib.parse
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
        password = b'fixture password for local HTTPS only'
        bootstrapped = json.loads(subprocess.check_output(
            [str(binary), '--config', str(config_path), '--bootstrap-sysop', 'operator'],
            input=password + b'\n', timeout=15,
            creationflags=subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0,
        ))
        assert bootstrapped['username'] == 'operator'
        initial_audit=[]
        if isolated:
            text=init_trace.read_text(encoding='utf-8')
            initial_audit.append(audit.audit_trace(text,require_listener=False))
            (state_root/f'runtime-{"ipv6" if family==socket.AF_INET6 else "ipv4"}-initialization.trace').write_text(text,encoding='utf-8')
        subprocess.run([sys.executable, str(ROOT/'spikes/m00/tools/init_tls.py'), '--state', str(state)], check=True, capture_output=True)
        context = ssl.create_default_context(cafile=str(state/'tls/ca.pem'))
        cookies = http.cookiejar.CookieJar()
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), urllib.request.HTTPSHandler(context=context), urllib.request.HTTPCookieProcessor(cookies))
        def request(path, method='GET', headers=None, data=None):
            return opener.open(urllib.request.Request(origin+path, method=method, headers=headers or {}, data=data), timeout=3)
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
                    assert 'Runtime fixture &lt;node&gt;' in html
                    assert 'href="/login/"' in html and '<script src="/assets/shell-state.js" defer></script>' in html
                with request('/assets/shell.css') as response:
                    assert b'color-scheme: dark' in response.read()
                with request('/assets/shell-state.js') as response:
                    assert b'Node unreachable.' in response.read()
                with request('/status/') as response:
                    html = response.read().decode()
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
                session_path = '/api/v1/session'
                try:
                    request(session_path)
                    raise AssertionError('Read accepted without a current session')
                except urllib.error.HTTPError as error:
                    assert error.code == 401
                credentials = json.dumps(dict(username='operator', password=password.decode())).encode()
                with request(session_path, 'POST', {'Origin': origin, 'Content-Type': 'application/json'}, credentials) as response:
                    assert json.load(response)['role'] == 'sysop'
                with request(session_path) as response:
                    assert json.load(response)['username'] == 'operator'
                proof = next(cookie.value for cookie in cookies if cookie.name == '__Host-bbsbox_csrf')
                if attempt == 1:
                    with request(session_path, 'DELETE', {'Origin': origin, 'X-BBSBOX-CSRF': proof}) as response:
                        assert response.status == 204
                    assert not any(cookie.name.startswith('__Host-bbsbox_') for cookie in cookies)
                    form_headers = {'Origin': origin, 'Content-Type': 'application/x-www-form-urlencoded'}
                    encoded = urllib.parse.urlencode(dict(username='operator', password=password.decode())).encode()
                    with request('/login/', 'POST', form_headers, encoded) as response:
                        account = response.read().decode()
                        assert response.geturl().endswith('/account/') and '>operator<' in account
                        assert response.headers['Cache-Control'] == 'no-store'
                    hidden = re.search(r'name="csrf" value="([0-9a-f]{64})"', account)
                    assert hidden is not None
                    theme = dict(revision=0, csrf=hidden.group(1), background='#111111', text='#FFFFFF', accent='#73F7AB', secondary='#FEC566')
                    encoded_theme = urllib.parse.urlencode(theme).encode()
                    with request('/settings/theme/preview/', 'POST', form_headers, encoded_theme) as response:
                        preview = response.read().decode()
                        url = re.search(r'href="(/assets/shell.css\?preview=[^" ]+)"', preview)
                        assert url is not None
                    with request(url.group(1)) as response:
                        assert b'--background: #111111' in response.read()
                    with request('/assets/shell.css') as response:
                        assert b'--background: #111111' not in response.read()
                    with request('/settings/theme/apply/', 'POST', form_headers, encoded_theme) as response:
                        assert response.geturl().endswith('/settings/theme/')
                    with request('/assets/shell.css') as response:
                        assert b'--background: #111111' in response.read()
                    theme['revision'] = 1
                    with request('/settings/theme/reset/', 'POST', form_headers, urllib.parse.urlencode(theme).encode()) as response:
                        assert response.geturl().endswith('/settings/theme/')
                    with request('/assets/shell.css') as response:
                        assert b'--background: #111111' not in response.read()
                    site = dict(revision=0, csrf=hidden.group(1), welcome='Neighbors <welcome>', artwork='<text art>', menu='status,home,account')
                    with request('/settings/site/', 'POST', form_headers, urllib.parse.urlencode(site).encode()) as response:
                        assert response.geturl().endswith('/settings/site/')
                    with request('/') as response:
                        updated = response.read().decode()
                        assert 'Neighbors &lt;welcome&gt;' in updated and '&lt;text art&gt;' in updated
                        menu = updated.split('<nav aria-label="Main">', 1)[1].split('</nav>', 1)[0]
                        assert re.findall(r'<a href="([^"]+)"', menu) == ['/status/', '/', '/account/', '/discussions/', '/mail/', '/directory/']
                    def discussion_proof(page):
                        return {key: re.search(r'name="'+key+r'" value="([0-9a-f]+)"', page).group(1) for key in ['csrf', 'request']}
                    def submit(path, fields):
                        return request(path, 'POST', form_headers, urllib.parse.urlencode(fields).encode())
                    with request('/discussions/') as response:
                        landing = response.read().decode()
                        assert 'No boards to show.' in landing
                    board_form = dict(discussion_proof(landing), title='General <board>', board='general')
                    with submit('/discussions/', board_form) as response:
                        assert response.geturl().endswith('/discussions/general/')
                    with submit('/discussions/', board_form) as response:
                        assert response.geturl().endswith('/discussions/general/')
                    try:
                        submit('/discussions/', dict(board_form, title='Different'))
                        raise AssertionError('Changed retry was accepted')
                    except urllib.error.HTTPError as error:
                        assert error.code == 409
                    member = dict(username='Neighbor', password='fixture member passphrase', role='member', csrf=hidden.group(1))
                    with request('/settings/accounts/', 'POST', form_headers, urllib.parse.urlencode(member).encode()) as response:
                        assert response.status == 201 and b'Account created.' in response.read()
                    other = dict(member, username='Other', password='fixture second member passphrase')
                    with submit('/settings/accounts/', other) as response:
                        assert response.status == 201
                    try:
                        request('/events/')
                        raise AssertionError('Uninstalled events section was served')
                    except urllib.error.HTTPError as error:
                        assert error.code == 404 and b'Section unavailable' in error.read()
                    payload = urllib.parse.urlencode(dict(csrf=hidden.group(1))).encode()
                    with request('/account/sign-out/', 'POST', form_headers, payload) as response:
                        assert response.geturl() == origin + '/'
                    with request('/account/') as response:
                        assert response.geturl().endswith('/login/')
                    member_login = urllib.parse.urlencode({key: member[key] for key in ['username', 'password']}).encode()
                    with request('/login/', 'POST', form_headers, member_login) as response:
                        account = response.read()
                        assert response.geturl().endswith('/account/') and b'>neighbor<' in account
                        assert b'/settings/accounts/' not in account
                    try:
                        request('/settings/accounts/')
                        raise AssertionError('Member could open the SysOp account editor')
                    except urllib.error.HTTPError as error:
                        assert error.code == 403
                    with request('/discussions/general/') as response:
                        page = response.read().decode()
                    thread_form = dict(discussion_proof(page), title='Library meetup', text='*Hi* <script>bad</script> ![alt](https://outside.invalid/picture) [files](/files/)\n\n' + 'å' * 6000)
                    with submit('/discussions/general/new/', thread_form) as response:
                        thread_url = urllib.parse.urlsplit(response.geturl()).path
                        thread = response.read().decode()
                        assert '<em>Hi</em>' in thread and '&lt;script&gt;' in thread
                        assert '<img' not in thread and 'href="/files/"' in thread
                    with submit('/discussions/general/new/', thread_form) as response:
                        assert urllib.parse.urlsplit(response.geturl()).path == thread_url
                    with request('/discussions/general/') as response:
                        assert response.read().count(b'Library meetup') == 1
                    parent = thread_url.rstrip('/').rsplit('/', 1)[1]
                    reply_form = dict(discussion_proof(thread), parent=parent, text='Meet at six.')
                    with submit(thread_url + 'reply/', reply_form) as response:
                        reply_url = urllib.parse.urlsplit(response.geturl())
                        reply_id = urllib.parse.parse_qs(reply_url.query)['focus'][0]
                        assert b'Meet at six.' in response.read()
                    try:
                        submit(thread_url + 'reply/', dict(reply_form, csrf='0' * 64))
                        raise AssertionError('Receipt replay bypassed CSRF')
                    except urllib.error.HTTPError as error:
                        assert error.code == 403
                    edit_path = f'{thread_url}{reply_id}/edit/'
                    withdraw_path = f'{thread_url}{reply_id}/withdraw/'
                    def revision_form(page, path):
                        form = page.split(f'<form action="{path}"', 1)[1].split('</form>', 1)[0]
                        previous = re.search(r'name="previous" value="([0-9a-f]{64})"', form).group(1)
                        return dict(discussion_proof(form), previous=previous)
                    with request(edit_path) as response:
                        editor = response.read().decode()
                    with submit(edit_path, dict(revision_form(editor, edit_path), text='Meet at seven.')) as response:
                        updated = response.read()
                        assert b'Meet at seven.' in updated and b'Meet at six.' not in updated
                    try:
                        submit(withdraw_path, revision_form(editor, withdraw_path))
                        raise AssertionError('Stale revision was withdrawn')
                    except urllib.error.HTTPError as error:
                        assert error.code == 409
                    with request(edit_path) as response:
                        editor = response.read().decode()
                    with submit(withdraw_path, revision_form(editor, withdraw_path)) as response:
                        withdrawn = response.read()
                        assert b'Post withdrawn.' in withdrawn and b'Meet at seven.' not in withdrawn
                    def sign_out():
                        csrf = next(cookie.value for cookie in cookies if cookie.name == '__Host-bbsbox_csrf')
                        with submit('/account/sign-out/', dict(csrf=csrf)) as response:
                            assert response.geturl() == origin + '/'

                    def sign_in(username, password):
                        with submit('/login/', dict(username=username, password=password)) as response:
                            assert response.geturl().endswith('/account/')

                    with request('/mail/') as response:
                        mailbox = response.read().decode()
                        assert 'No conversations to show.' in mailbox
                    send_form = dict(discussion_proof(mailbox), recipient='Other', subject='Private meetup', text='*Only the recipients* can read this.')
                    with submit('/mail/', send_form) as response:
                        private_url = urllib.parse.urlsplit(response.geturl()).path
                        assert response.headers['Cache-Control'] == 'no-store'
                        assert b'<em>Only the recipients</em>' in response.read()
                    with submit('/mail/', send_form) as response:
                        assert urllib.parse.urlsplit(response.geturl()).path == private_url
                    with request('/mail/contact/') as response:
                        contact = response.read().decode()
                        assert '<option value="operator">operator</option>' in contact
                        contact_form = dict(discussion_proof(contact), recipient='operator', subject='Operator help', text='Please check the local sign.')
                    with submit('/mail/contact/', contact_form) as response:
                        contact_url = urllib.parse.urlsplit(response.geturl()).path
                        assert b'Please check the local sign.' in response.read()
                    sign_out()
                    with request(private_url) as response:
                        assert response.geturl().endswith('/login/')
                    sign_in('operator', password.decode())
                    try:
                        request(private_url)
                        raise AssertionError('Unaddressed SysOp could read mail in the browser')
                    except urllib.error.HTTPError as error:
                        assert error.code == 404
                        assert b'Private meetup' not in error.read()
                    with request('/mail/') as response:
                        inbox = response.read()
                        assert b'Operator help' in inbox and b'Private meetup' not in inbox
                    with request(contact_url) as response:
                        assert b'Please check the local sign.' in response.read()
                    sign_out()
                    sign_in(other['username'], other['password'])
                    with request(private_url) as response:
                        conversation = response.read().decode()
                        assert '<em>Only the recipients</em>' in conversation
                    parent = re.search(r'name="parent" value="([0-9a-f]{64})"', conversation).group(1)
                    with submit(private_url + 'reply/', dict(discussion_proof(conversation), parent=parent, text='See you there.')) as response:
                        assert b'See you there.' in response.read()
                    sign_out()
                    sign_in(member['username'], member['password'])
                    with request(private_url) as response:
                        assert b'See you there.' in response.read()
                    with request('/directory/') as response:
                        directory = response.read()
                        for username in ('neighbor', 'operator', 'other'):
                            assert f'href="/directory/{username}/"'.encode() in directory
                    with request('/directory/?username=Neighbor') as response:
                        assert response.geturl().endswith('/directory/neighbor/')
                    def profile_form():
                        with request('/account/profile/') as response:
                            page = response.read().decode()
                        return {key: re.search(r'name="'+key+r'" value="([0-9a-f]+)"', page).group(1) for key in ('csrf', 'revision')}

                    profile_details = dict(bio='Meet me at <the library>', message='Private short message', email='fixture@example.invalid', phone='555-fixture', contact='Fixture private address', show_bio='on', show_phone='on')
                    first = dict(profile_details, **profile_form())
                    with submit('/account/profile/', first) as response:
                        assert b'Meet me' not in response.read()
                    publish = dict(profile_details, **profile_form(), public='on')
                    with submit('/account/profile/', publish) as response:
                        assert b'Meet me at &lt;the library&gt;' in response.read()
                    try:
                        submit('/account/profile/', first)
                        raise AssertionError('Stale profile update was accepted')
                    except urllib.error.HTTPError as error:
                        assert error.code == 409
                    sign_out()
                    with request('/directory/neighbor/') as response:
                        assert response.geturl().endswith('/login/')
                    sign_in('operator', password.decode())
                    with request('/directory/neighbor/') as response:
                        public_profile = response.read()
                        assert response.headers['Cache-Control'] == 'no-store'
                        assert b'Meet me at &lt;the library&gt;' in public_profile
                        assert b'555-fixture' in public_profile
                        for hidden_value in (b'fixture@example.invalid', b'Fixture private address', b'Private short message'):
                            assert hidden_value not in public_profile
                    sign_out()
                    sign_in(member['username'], member['password'])
                    hide = dict(profile_details, **profile_form())
                    with submit('/account/profile/', hide) as response:
                        assert b'555-fixture' not in response.read()
                    sign_out()
                    sign_in(other['username'], other['password'])
                    with request('/directory/neighbor/') as response:
                        private_profile = response.read()
                        assert b'Meet me' not in private_profile and b'555-fixture' not in private_profile
                    with request('/directory/') as response:
                        assert b'href="/directory/neighbor/"' in response.read()
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
        return dict(family='IPv6' if family == socket.AF_INET6 else 'IPv4', simultaneous_dual_stack=True, tls=True, untrusted_ca_rejected=True, local_assets=4, identity_preserved_after_restart=True, configured_alias_accepted=True, unexpected_authority_rejected=True, public_status_read_only=True, local_session_lifecycle=True, restart_invalidates_session=True, palette_preview_apply_reset=True, welcome_and_menu_settings=True, member_onboarding=True, unavailable_section_status=True, local_board_thread_reply=True, discussion_retry_and_csrf=True, discussion_edit_and_withdraw=True, local_mail_participants=True, directory_and_profile_privacy=True, initialization_audit=initial_audit, network_audit=traces)


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
