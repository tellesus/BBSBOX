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
import time
import unittest
from unittest.mock import patch

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'deploy'))
import update_release as update
from restore_provisioning import check_closed_data

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
    def test_serving_requires_initialized_registered_identity_and_cannot_recreate_a_missing_installed_key(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();database=root/'node.sqlite';config=root/'node.json'
            run(sys.executable,ROOT/'spikes/m00/tools/init_tls.py','--state',root)
            config.write_text(json.dumps(dict(format='bbsbox.node-config/v1',name='Identity fixture',bind='127.0.0.1:8443',origin='https://localhost:8443',database=str(database),certificate=str(root/'tls/server.pem'),certificate_key=str(root/'tls/server-key.pem'))),encoding='utf-8')
            missing=subprocess.run([str(BASE),'--config',str(config)],capture_output=True,timeout=5)
            self.assertNotEqual(missing.returncode,0);self.assertFalse(database.exists())
            node=json.loads(run(BASE,'--config',config,'--initialize').stdout)['node_id']
            installed=root/'installation.json';installed.write_text(json.dumps(dict(format='bbsbox.install/v1',node_id='node:'+'11'*32)),encoding='utf-8')
            wrong=run(BASE,'--config',config,check=False)
            self.assertNotEqual(wrong.returncode,0);self.assertIn(b'identity differs',wrong.stderr)
            installed.write_text(json.dumps(dict(format='bbsbox.install/v1',node_id=node)),encoding='utf-8')
            with closing(sqlite3.connect(database)) as connection:
                connection.execute('DELETE FROM identity_node_identity');connection.commit()
            self.assertNotEqual(run(BASE,'--config',config,'--initialize',check=False).returncode,0)
            with closing(sqlite3.connect(database)) as connection:self.assertEqual(connection.execute('SELECT count(*) FROM identity_node_identity').fetchone()[0],0)

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
            with closing(sqlite3.connect(database)) as connection:self.assertEqual(connection.execute('SELECT count(*) FROM core_migrations').fetchone()[0],21)
            migrated=json.loads(run(SUCCESS,'migrate-existing','--database',database,'--node',node).stdout)
            self.assertFalse(migrated['listener_opened'])
            with closing(sqlite3.connect(database)) as connection:
                self.assertEqual(connection.execute('SELECT count(*) FROM core_migrations').fetchone()[0],23)
                self.assertEqual(connection.execute('SELECT count(*) FROM events_upgrade_receipts').fetchone()[0],2)
            self.assertNotEqual(run(BASE,'migrate-existing','--database',database,'--node',node,check=False).returncode,0)
            compatible=root/'compatible'
            resumed=json.loads(run(SUCCESS,'restore-compatible','--archive',cipher,'--node',node,'--identity-file',key,'--output',compatible,'--staging',root,'--source','lost','--reason','Simulated lost source; private isolated continuation','--at','0').stdout)
            self.assertEqual(resumed['format'],'bbsbox.compatible-continuation/v1');self.assertTrue(resumed['migration_required']);self.assertEqual(len(resumed['source_registry']),21);self.assertFalse(resumed['installed'])
            with closing(sqlite3.connect(compatible/'data/node.sqlite')) as connection:self.assertEqual(connection.execute('SELECT count(*) FROM core_migrations').fetchone()[0],21)
            run(SUCCESS,'migrate-existing','--database',compatible/'data/node.sqlite','--node',node)
            check_closed_data(compatible/'data')
            with closing(sqlite3.connect(compatible/'data/node.sqlite')) as connection:self.assertEqual(connection.execute('SELECT count(*) FROM core_migrations').fetchone()[0],23)
            target=root/'restored'
            restored=json.loads(run(BASE,'restore-and-authorize','--archive',cipher,'--node',node,'--identity-file',key,'--output',target,'--staging',root,'--source','retired','--reason','Stopped disposable upgraded fixture','--at','0').stdout)
            self.assertEqual(saved['inventory_sha256'],restored['inventory_sha256']);self.assertTrue(restored['isolated_writer_ready']);self.assertFalse(restored['installed'])
            with closing(sqlite3.connect(target/'data/node.sqlite')) as connection:
                self.assertEqual(connection.execute('SELECT count(*) FROM core_migrations').fetchone()[0],21)
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
    @unittest.skipUnless(sys.platform=='linux' and (ROOT/'target/debug/examples/upgrade_interrupted').is_file(),'Actual Linux migration process and writer flock')
    def test_live_migration_process_excludes_replacement_without_changing_its_lock_inode(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();database=root/'node.sqlite';config=root/'node.json'
            config.write_text(json.dumps(dict(format='bbsbox.node-config/v1',name='Lock fixture',bind='127.0.0.1:8443',origin='https://localhost:8443',database=str(database),certificate=str(root/'missing.pem'),certificate_key=str(root/'missing-key.pem'))),encoding='utf-8')
            node=json.loads(run(BASE,'--config',config,'--initialize').stdout)['node_id']
            lock=root/'node.sqlite.writer.lock';inode=lock.stat().st_ino
            child=subprocess.Popen([str(ROOT/'target/debug/examples/upgrade_interrupted'),'migrate-existing','--database',str(database),'--node',node],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            try:
                with patch.object(update,'DATA',root):
                    deadline=time.monotonic()+5
                    while time.monotonic()<deadline:
                        self.assertIsNone(child.poll(),'Migration exited before exclusion was observed')
                        try:
                            with update.source_lock():pass
                        except BlockingIOError:break
                        time.sleep(.01)
                    else:self.fail('Did not observe the actual Rust migration writer lock')
                    child.kill();child.communicate(timeout=5)
                    with update.source_lock():self.assertEqual(lock.stat().st_ino,inode)
                with closing(sqlite3.connect(database)) as connection:self.assertEqual(connection.execute('SELECT count(*) FROM core_migrations').fetchone()[0],21)
                run(BASE,'migrate-existing','--database',database,'--node',node)
            finally:
                if child.poll() is None:child.kill();child.communicate(timeout=5)


if __name__=='__main__':unittest.main()
