# SPDX-License-Identifier: AGPL-3.0-or-later
"""Exercise a real two-origin HTTPS demo and its separate persistent worker."""
import argparse
import http.cookiejar
import json
import os
from pathlib import Path
import re
import socket
import sqlite3
import ssl
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
PASSWORD = 'local game test passphrase'


def fixture(binary, directory):
    with socket.socket() as reserve:
        reserve.bind(('127.0.0.1', 0))
        port = reserve.getsockname()[1]
    core = f'https://127.0.0.1:{port}'
    game = f'https://localhost:{port}'
    configuration = directory/'configuration'
    configuration.mkdir(mode=0o700)
    data = directory/'data'
    data.mkdir(mode=0o700)
    config = configuration/'node.json'

    def native(path):
        return '\\\\?\\'+str(path) if os.name == 'nt' else str(path)

    config.write_text(json.dumps(dict(
        format='bbsbox.node-config/v1', name='Game fixture', bind=f'127.0.0.1:{port}',
        additional_binds=[f'[::1]:{port}'], origin=core, game_origin=game,
        database=native(data/'node.sqlite'), certificate=native(configuration/'tls/server.pem'),
        certificate_key=native(configuration/'tls/server-key.pem'),
    )), encoding='utf-8')
    command = [str(binary), '--config', str(config)]
    flags = subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0
    initialized = subprocess.run([*command, '--initialize'], check=True, capture_output=True, creationflags=flags)
    node = json.loads(initialized.stdout)['node_id']
    subprocess.run(
        [*command, '--bootstrap-sysop', 'operator'], input=(PASSWORD+'\n').encode(),
        check=True, capture_output=True, creationflags=flags,
    )
    subprocess.run(
        [sys.executable, str(ROOT/'spikes/m00/tools/init_tls.py'), '--state', str(configuration)],
        check=True, capture_output=True, creationflags=flags,
    )
    (configuration/'installation.json').write_text(json.dumps(dict(
        format='bbsbox.development-install/v1', node_id=node,
    )), encoding='utf-8')
    context = ssl.create_default_context(cafile=str(configuration/'tls/ca.pem'))

    def operator(*arguments, accept=True):
        result = subprocess.run([str(binary), *map(str, arguments)], capture_output=True,
                                creationflags=flags, timeout=30)
        if accept:
            assert result.returncode == 0, result.stderr.decode(errors='replace')
            return json.loads(result.stdout)
        assert result.returncode != 0, result.stdout

    identity = directory/'backup.key'
    recipient = operator('backup-keygen', '--output', identity)['recipient']
    archive = directory/'game.age'
    backup = ['--config', config, '--node', node, '--recipient', recipient,
              '--output', archive, '--staging', directory]
    operator('backup-game-node', *backup, accept=False)
    assert not archive.exists() and not (data/'games').exists()
    uninitialized = data/'games'/'dungeon-demo'/'state.sqlite'
    uninitialized.parent.mkdir(parents=True)
    uninitialized.touch()
    operator('backup-game-node', *backup, accept=False)
    assert not archive.exists() and uninitialized.stat().st_size == 0
    uninitialized.unlink()

    class Client:
        def __init__(self):
            self.opener = urllib.request.build_opener(
                urllib.request.ProxyHandler({}), urllib.request.HTTPSHandler(context=context),
                urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()),
            )

        def request(self, origin, path, value=None, headers=None):
            headers = dict(headers or {})
            if value is not None:
                headers.setdefault('Origin', origin)
                if origin == core:
                    headers['Content-Type'] = 'application/x-www-form-urlencoded'
                    data = urllib.parse.urlencode(value).encode()
                else:
                    headers['Content-Type'] = 'application/json'
                    data = json.dumps(value).encode()
            else:
                data = None
            return self.opener.open(urllib.request.Request(origin+path, data=data, headers=headers), timeout=4)

        def csrf(self, path):
            with self.request(core, path) as response:
                return re.search(rb'name="csrf" value="([0-9a-f]{64})"', response.read()).group(1).decode()

        def login(self, name):
            with self.request(core, '/login/', dict(username=name, password=PASSWORD)) as response:
                assert response.geturl() == core+'/account/'

        def launch(self):
            with self.request(core, '/games/') as response:
                assert "form-action 'self' " + game in response.headers['Content-Security-Policy']
            with self.request(core, '/games/', dict(csrf=self.csrf('/games/'))) as response:
                assert response.geturl().startswith(game+'/#ticket=')
                assert b'Dungeon demo' in response.read()
                assert response.headers['Cache-Control'] == 'no-store'
                assert 'Set-Cookie' not in response.headers
                return urllib.parse.parse_qs(urllib.parse.urlsplit(response.geturl()).fragment)['ticket'][0]

        def exchange(self, ticket):
            with self.request(game, '/game/v1/exchange', dict(ticket=ticket)) as response:
                assert 'Set-Cookie' not in response.headers
                return json.load(response)

    def refused(code, action):
        try:
            with action():
                raise AssertionError(f'Expected refusal {code}')
        except urllib.error.HTTPError as error:
            assert error.code == code, error.code

    for attempt in range(2):
        process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, creationflags=flags)
        try:
            ready = Client()
            for _ in range(120):
                if process.poll() is not None:
                    raise RuntimeError(process.communicate()[1].decode(errors='replace')[:1000])
                try:
                    with ready.request(core, '/api/v1/node') as response:
                        assert json.load(response)['status'] == 'ready'
                        break
                except urllib.error.URLError:
                    time.sleep(.05)
            else:
                raise AssertionError('The development game node did not start')
            if attempt == 0:
                operator('backup-game-node', *backup, accept=False)
                assert not archive.exists()
            one = Client()
            two = Client()
            third = Client()
            one.login('operator')
            if attempt == 0:
                for username in ('player', 'neighbor'):
                    csrf = one.csrf('/settings/accounts/')
                    with one.request(core, '/settings/accounts/', dict(username=username, password=PASSWORD, csrf=csrf, role='member')) as response:
                        assert response.status == 201
                with one.request(core, '/groups/new/') as response:
                    html = response.read().decode()
                proof = {key: re.search(r'name="'+key+r'" value="([0-9a-f]+)"', html).group(1) for key in ('csrf','request')}
                with one.request(core, '/groups/new/', dict(proof, name='Game cohort', kind='cohort', description='')) as response:
                    assert b'Game cohort' in response.read()
            two.login('player')
            third.login('neighbor')
            refused(404, lambda: one.request(game, '/account/'))
            refused(404, lambda: one.request(core, '/game/v1/state'))
            refused(403, lambda: one.request(game, '/', headers={'Cookie': '__Host-bbsbox_session=fixture'}))
            ticket = one.launch()
            refused(403, lambda: one.request(game, '/game/v1/exchange', dict(ticket=ticket), {'Origin': core}))
            first = one.exchange(ticket)
            refused(401, lambda: one.request(game, '/game/v1/exchange', dict(ticket=ticket)))
            second = two.exchange(two.launch())
            assert first['state']['seat'] == 0 and second['state']['seat'] == 1
            assert first['state']['chamber'] == attempt
            for client, grant, expected in ((one, first, ['Game cohort']), (two, second, [])):
                with client.request(game, '/game/v1/groups', headers={'Authorization': 'Bearer '+grant['session']}) as response:
                    assert [group['name'] for group in json.load(response)['memberships']] == expected
            if attempt == 0:
                with third.request(core, '/directory/operator/') as response:
                    assert b'Dungeon demo' not in response.read()
                with one.request(core, '/account/profile/') as response:
                    html = response.read().decode()
                proof = {key: re.search(r'name="'+key+r'" value="([0-9a-f]+)"', html).group(1) for key in ('csrf','revision')}
                with one.request(core, '/account/profile/', dict(proof, public='on', show_games='on', bio='', message='', email='', phone='', contact='')) as response:
                    assert b'Explorer 1' in response.read()
                with third.request(core, '/directory/operator/') as response:
                    assert b'Explorer 1' in response.read()
            refused(409, lambda: third.request(game, '/game/v1/exchange', dict(ticket=third.launch())))
            with third.request(core, '/discussions/') as response:
                assert b'Discussions' in response.read()
            client, session = (one, first) if attempt == 0 else (two, second)
            proof = {'Authorization': 'Bearer '+session['session']}
            move = dict(command=f'{attempt+1:032x}', action='explore')
            for _ in range(2):
                with client.request(game, '/game/v1/play', move, proof) as response:
                    state = json.load(response)
                    assert state['chamber'] == attempt+1
            refused(401, lambda: Client().request(game, '/game/v1/state'))
        finally:
            if process.poll() is None:
                process.terminate()
            try:
                process.communicate(timeout=12)
            except subprocess.TimeoutExpired:
                process.kill()
                process.communicate()

    operator('backup', *backup, accept=False)
    assert not archive.exists()
    # The game stays recoverable after it is disabled. A different, unregistered game blocks capture.
    settings = json.loads(config.read_text(encoding='utf-8'))
    del settings['game_origin']
    config.write_text(json.dumps(settings), encoding='utf-8')
    unknown = data/'games'/'unknown'
    unknown.mkdir()
    operator('backup-game-node', *backup, accept=False)
    assert not archive.exists()
    unknown.rmdir()
    sealed = operator('backup-game-node', *backup)
    assert sealed['scope'] == 'game-node' and sealed['game_checkpoints'] == 1
    restored = directory/'restored'
    restore = ['--archive', archive, '--node', node, '--identity-file', identity,
               '--output', restored, '--staging', directory]
    operator('restore', *restore, accept=False)
    assert not restored.exists()
    opened = operator('restore-game-node', *restore)
    assert opened['inventory_sha256'] == sealed['inventory_sha256']
    assert opened['state'] == 'recovery-required' and opened['game_checkpoints'] == 1
    assert json.loads((restored/'game-recovery.json').read_text())['state'] == 'recovery-required'
    assert Path(opened['database']).is_file() and not (restored/'.restore-incomplete').exists()
    game_db = restored/'games'/'dungeon-demo'/'state.sqlite'
    db = sqlite3.connect(game_db.as_uri()+'?mode=ro', uri=True)
    try:
        assert db.execute('SELECT chamber FROM dungeon').fetchone() == (2,)
        assert db.execute('SELECT count(*) FROM players').fetchone() == (2,)
        assert db.execute('SELECT count(*) FROM receipts').fetchone() == (2,)
    finally:
        db.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT/('target/debug/bbsbox-node.exe' if os.name == 'nt' else 'target/debug/bbsbox-node'))
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='bbsbox-games-') as state:
        fixture(args.binary.resolve(), Path(state).resolve())
    print('PASS: separate HTTPS game host, two players, restart and encrypted game-node recovery')


if __name__ == '__main__':
    main()
