# SPDX-License-Identifier: AGPL-3.0-or-later
"""Probe the game socket and its worker in the disposable signed Debian guest."""
import json
import os
from pathlib import Path
import pwd
import socket
import stat
import struct
import subprocess
import sys
import time

SOCKET = '/run/bbsbox-game/worker.sock'
CLIENT = '/usr/local/lib/bbsbox-game-worker-probe.py'


def connect():
    connection = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    connection.settimeout(5)
    connection.connect(SOCKET)
    return connection


def request(connection, incoming, number, op, **arguments):
    identifier = [number] * 16
    data = json.dumps(dict(version=1, id=identifier, operation=dict(op=op, **arguments))).encode()
    connection.sendall(struct.pack('!I', len(data)) + data)
    size, = struct.unpack('!I', incoming.read(4))
    assert 0 < size <= 65536
    reply = json.loads(incoming.read(size))
    assert reply['id'] == identifier and reply['version'] == 1
    return reply['result']


def play():
    with connect() as connection, connection.makefile('rb') as incoming:
        def call(number, op, **arguments):
            return request(connection, incoming, number, op, **arguments)

        assert call(1, 'describe')['manifest']['game_id'] == 'dungeon-demo'
        assert call(2, 'join', player=[1]*16)['state']['seat'] == 0
        assert call(3, 'join', player=[2]*16)['state']['seat'] == 1
        assert call(4, 'act', player=[1]*16, command=[9]*16, action='explore')['state']['chamber'] == 1
        assert call(5, 'pause', fence=[8]*16) == {'result':'refused', 'reason':'unsupported'}
        print('ready', flush=True)
        sys.stdin.read()


def run(*args):
    return subprocess.check_output(args, text=True, timeout=10).strip()


def denied():
    try:
        with connect():
            raise AssertionError('Unexpected socket access')
    except PermissionError:
        pass


def probe():
    account = pwd.getpwnam('bbsbox')
    worker = pwd.getpwnam('bbsbox-game')
    assert account.pw_uid != worker.pw_uid
    metadata = Path(SOCKET).lstat()
    assert stat.S_ISSOCK(metadata.st_mode) and stat.S_IMODE(metadata.st_mode) == 0o600
    assert metadata.st_uid == account.pw_uid
    run('/usr/sbin/runuser', '-u', 'nobody', '--', '/usr/bin/python3', CLIENT, 'denied')
    with subprocess.Popen(['/usr/sbin/runuser', '-u', 'bbsbox', '--', '/usr/bin/python3', CLIENT, 'play'],
                          stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True) as client:
        try:
            assert client.stdout.readline().strip() == 'ready'
            units = run('/usr/bin/systemctl', 'list-units', '--state=running', '--no-legend', '--plain', 'bbsbox-game-worker@*.service')
            unit, = [line.split()[0] for line in units.splitlines()]
            pid = int(run('/usr/bin/systemctl', 'show', unit, '-p', 'MainPID', '--value'))
            proc = Path('/proc')/str(pid)
            status = dict(line.split(':', 1) for line in (proc/'status').read_text().splitlines())
            assert set(map(int, status['Uid'].split())) == {worker.pw_uid}
            assert status['NoNewPrivs'].strip() == '1' and int(status['CapEff'], 16) == 0
            assert status['Seccomp'].strip() == '2'
            assert not os.path.samefile('/proc/self/ns/net', proc/'ns/net')
            for hidden in ('etc/bbsbox/node.json', 'var/lib/bbsbox/node.sqlite', 'var/lib/bbsbox-maintenance/release-control'):
                assert not (proc/'root'/hidden).exists(), hidden
            state = Path('/var/lib/bbsbox-game/state.sqlite')
            assert state.stat().st_uid == worker.pw_uid
            run('/usr/sbin/runuser', '-u', 'bbsbox', '--', '/usr/bin/test', '!', '-r', str(state))
        finally:
            client.stdin.close()
        assert client.wait(timeout=5) == 0
    for _ in range(50):
        if not proc.exists():
            break
        time.sleep(0.05)
    else:
        raise AssertionError('Worker stayed up after the core socket closed')
    with connect() as connection, connection.makefile('rb') as incoming:
        assert request(connection, incoming, 1, 'join', player=[1]*16) == {'result':'refused', 'reason':'paused'}
    # The backup uses a new connection after the root permission probe has been collected.
    for _ in range(50):
        units = run('/usr/bin/systemctl', 'list-units', '--state=active,activating', '--no-legend', '--plain', 'bbsbox-game-worker@*.service')
        if not units:
            break
        time.sleep(0.05)
    else:
        raise AssertionError('Maintenance probe did not release its socket')
    print(json.dumps(dict(gameplay=True, separate_worker_account=True, core_data_hidden=True,
                          network_namespace_isolated=True, unrelated_account_denied=True, worker_exits_on_disconnect=True,
                          gameplay_and_checkpoint_roles_separated=True)))


if __name__ == '__main__':
    {'probe': probe, 'play': play, 'denied': denied}[sys.argv[1]]()
