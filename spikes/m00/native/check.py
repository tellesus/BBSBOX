# SPDX-License-Identifier: AGPL-3.0-or-later
"""Run actual bundled WebViews across create, restart and explicit backup recovery."""
import argparse
from contextlib import closing
import json
import os
from pathlib import Path
import re
import sqlite3
import subprocess
import tempfile
from urllib.parse import urlsplit

ROOT=Path(__file__).resolve().parents[3]


def check(binary, installed_store=None):
    (ROOT/'work').mkdir(exist_ok=True)
    binary=binary.resolve(strict=True)
    command=[str(binary)] if installed_store is None else [str(binary),'run',str(installed_store.resolve(strict=True)),'--']
    with tempfile.TemporaryDirectory(prefix='native-client-',dir=ROOT/'work') as temporary:
        directory=Path(temporary)
        password=directory/'local.pass'
        password.write_text('native local fixture passphrase',encoding='utf-8');password.chmod(0o600)
        backup_password=directory/'backup.pass'
        backup_password.write_text('native backup fixture passphrase',encoding='utf-8');backup_password.chmod(0o600)
        vault=directory/'identity.sqlite';recovered=directory/'recovered.sqlite';backup=directory/'backup.kit'

        def run(name,action,path,expect=True,**options):
            request=directory/(name+'-request.json');output=directory/(name+'-report.json')
            request.write_text(json.dumps(dict(action=action,vault=str(path),passphrase_file=str(password),**options)),encoding='utf-8')
            flags=subprocess.CREATE_NO_WINDOW if os.name=='nt' else 0
            result=subprocess.run([*command,'--check',str(output),'--request',str(request)],
                                  capture_output=True,timeout=120 if installed_store else 45,creationflags=flags)
            if not expect:
                assert result.returncode!=0 and not output.exists()
                return
            if result.returncode:raise RuntimeError(result.stderr[-4096:].decode(errors='replace'))
            report=json.loads(output.read_text(encoding='utf-8'))
            assert report['format']=='bbsbox.native-client-probe/v1'
            assert re.fullmatch(r'person:[0-9a-f]{64}',report['person'])
            assert re.fullmatch(r'credential:[0-9a-f]{64}',report['device'])
            assert report['native_identity_command'] is True and report['remote_navigation_denied'] is True
            assert report['state_verified'] is True and report['key_persistence']=='encrypted-local-vault/v1'
            url=urlsplit(report['bundled_url'])
            assert (url.scheme,url.hostname) in [('tauri','localhost'),('http','tauri.localhost')]
            assert report['signed_distribution']=='pending'
            return report

        created=run('create','create',vault,backup_output=str(backup),backup_passphrase_file=str(backup_password))
        restarted=run('restart','unlock',vault)
        assert created['person']==restarted['person'] and created['device']==restarted['device']
        assert created['control_incarnation']!=restarted['control_incarnation']
        assert restarted['revision']==2 and restarted['requires_rejoin'] is False
        with closing(sqlite3.connect(vault)) as database:
            original=database.execute('SELECT revision,record FROM client_vault').fetchone()
        password.write_text('an incorrect native fixture passphrase',encoding='utf-8')
        run('wrong-pass','unlock',vault,expect=False)
        password.write_text('native local fixture passphrase',encoding='utf-8')
        with closing(sqlite3.connect(vault)) as database:
            assert database.execute('SELECT revision,record FROM client_vault').fetchone()==original
        run('after-wrong-pass','unlock',vault)
        restored=run('recover','recover',recovered,backup=str(backup),backup_passphrase_file=str(backup_password))
        reopened=run('recover-restart','unlock',recovered)
        assert restored['person']==created['person'] and restored['device']!=created['device']
        assert restored['device']==reopened['device'] and reopened['requires_rejoin'] is True
        assert reopened['revision']==1
        print(json.dumps(dict(format='bbsbox.native-client-persistence-check/v1',
            create_restart=True,device_preserved=True,wrong_pass_preserves_identity=True,
            backup_recovers_same_person=True,recovery_replaces_device=True,
            receive_only_guard_survives_restart=True,remote_navigation_denied=True),indent=2))


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,required=True)
    parser.add_argument('--installed-store',type=Path)
    args=parser.parse_args()
    check(args.binary,args.installed_store)
