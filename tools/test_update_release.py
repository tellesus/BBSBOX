# SPDX-License-Identifier: AGPL-3.0-or-later
"""Actual compiled upgrade assemblies plus portable admission guards; Debian activation is separate."""
import json
from contextlib import closing
import os
from pathlib import Path
import sqlite3
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'deploy'))
import update_release as update

SUFFIX='.exe' if os.name=='nt' else ''
BASE=ROOT/'target/debug'/('bbsbox-node'+SUFFIX)
SUCCESS=ROOT/'target/debug/examples'/('upgrade_successor'+SUFFIX)
REJECT=ROOT/'target/debug/examples'/('upgrade_rejecting'+SUFFIX)


def run(*args,check=True):
    result=subprocess.run([str(arg) for arg in args],capture_output=True,timeout=120)
    if check and result.returncode:raise AssertionError(result.stderr[-4096:].decode(errors='replace'))
    return result


@unittest.skipUnless(all(p.is_file() for p in [BASE,SUCCESS,REJECT]),'Build actual node-runtime binaries/examples first')
class CompiledUpgradeTests(unittest.TestCase):
    def test_distinct_compiled_sql_migrates_real_receipts_and_matching_backup_restores_old_registry(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve()
            # Match Rust's canonical Windows representation; production appliance paths are Debian.
            if os.name=='nt':root=Path('\\\\?\\'+str(root))
            configuration=root/'configuration';configuration.mkdir();(configuration/'tls').mkdir()
            for name in ['ca.pem','ca-key.pem','server.pem','server-key.pem']:
                (configuration/'tls'/name).write_bytes(b'Fixture only; this is not TLS acceptance')
            database=root/'node.sqlite'
            config=dict(format='bbsbox.node-config/v1',name='Upgrade fixture',bind='127.0.0.1:8443',origin='https://localhost:8443',database=str(database),certificate=str(configuration/'tls/server.pem'),certificate_key=str(configuration/'tls/server-key.pem'))
            path=configuration/'node.json';path.write_text(json.dumps(config),encoding='utf-8')
            node=json.loads(run(BASE,'--config',path,'--initialize').stdout)['node_id']
            (configuration/'installation.json').write_text(json.dumps(dict(format='bbsbox.install/v1',node_id=node)),encoding='utf-8')
            with closing(sqlite3.connect(database)) as connection:
                connection.executemany('INSERT INTO core_command_receipts VALUES(?,?,?,?)',[(n,'shared',bytes(32),b'saved') for n in ['first','second']])
                connection.commit()
            key=root/'operator.key';public=json.loads(run(BASE,'backup-keygen','--output',key).stdout)
            cipher=root/'before.age'
            saved=json.loads(run(BASE,'backup','--config',path,'--node',node,'--recipient',public['recipient'],'--output',cipher,'--staging',root).stdout)
            self.assertNotEqual(run(REJECT,'migrate-existing','--database',database,'--node',node,check=False).returncode,0)
            with closing(sqlite3.connect(database)) as connection:self.assertEqual(connection.execute('SELECT count(*) FROM core_migrations').fetchone()[0],7)
            migrated=json.loads(run(SUCCESS,'migrate-existing','--database',database,'--node',node).stdout)
            self.assertFalse(migrated['listener_opened'])
            with closing(sqlite3.connect(database)) as connection:
                self.assertEqual(connection.execute('SELECT count(*) FROM core_migrations').fetchone()[0],9)
                self.assertEqual(connection.execute('SELECT count(*) FROM events_upgrade_receipts').fetchone()[0],2)
            self.assertNotEqual(run(BASE,'migrate-existing','--database',database,'--node',node,check=False).returncode,0)
            target=root/'restored'
            restored=json.loads(run(BASE,'restore-and-authorize','--archive',cipher,'--node',node,'--identity-file',key,'--output',target,'--staging',root,'--source','retired','--reason','Stopped disposable upgraded fixture','--at','0').stdout)
            self.assertEqual(saved['inventory_sha256'],restored['inventory_sha256']);self.assertTrue(restored['isolated_writer_ready']);self.assertFalse(restored['installed'])
            with closing(sqlite3.connect(target/'data/node.sqlite')) as connection:
                self.assertEqual(connection.execute('SELECT count(*) FROM core_migrations').fetchone()[0],7)
                self.assertEqual(connection.execute("SELECT count(*) FROM core_command_receipts WHERE response=?",(b'saved',)).fetchone()[0],2)
            run(BASE,'migrate-existing','--database',target/'data/node.sqlite','--node',node)

    def test_admission_uses_the_actual_compiled_prefix_and_refuses_changed_removed_or_unowned_sql(self):
        previous=json.loads(run(BASE,'--release-profile').stdout);successor=json.loads(run(SUCCESS,'--release-profile').stdout)
        # Portable builds report their actual platform; admission remains restricted to Debian.
        if sys.platform!='linux':
            with self.assertRaises(ValueError):update.compatible(previous,successor)
        previous['target']=successor['target']='debian13-amd64'
        update.compatible(previous,successor)
        with self.assertRaises(ValueError):update.compatible(successor,previous)
        for change in ['digest','missing','owner','schema']:
            altered=json.loads(json.dumps(successor))
            if change=='digest':altered['module_migrations'][0]['sha256']='00'*32
            if change=='missing':altered['module_migrations'].pop(0)
            if change=='owner':altered['module_migrations'].append(dict(module='unowned',version=1,sha256='11'*32))
            if change=='schema':altered['core_schema']+=1
            with self.subTest(change=change),self.assertRaises(ValueError):update.compatible(previous,altered)


class RecoveryExclusionTests(unittest.TestCase):
    @unittest.skipUnless(sys.platform=='linux','Same Linux flock as the storage writer')
    def test_live_migration_process_excludes_replacement_without_changing_its_lock_inode(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);lock=root/'node.sqlite.writer.lock';lock.write_bytes(b'preserved')
            child=subprocess.Popen([sys.executable,'-c',"import fcntl,sys; f=open(sys.argv[1],'r+b'); fcntl.flock(f,fcntl.LOCK_EX); print('locked',flush=True); sys.stdin.read()",str(lock)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            try:
                self.assertEqual(child.stdout.readline(),b'locked\n');inode=lock.stat().st_ino
                with patch.object(update,'DATA',root):
                    with self.assertRaises(BlockingIOError):
                        with update.source_lock():self.fail('Live migration admitted replacement')
                    child.communicate(timeout=5)
                    with update.source_lock():self.assertEqual(lock.stat().st_ino,inode)
                self.assertEqual(lock.read_bytes(),b'preserved')
            finally:
                if child.poll() is None:child.kill();child.communicate(timeout=5)


if __name__=='__main__':unittest.main()
