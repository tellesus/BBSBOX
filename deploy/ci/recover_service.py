# SPDX-License-Identifier: AGPL-3.0-or-later
"""Recover a cold source fixture onto separate prepared media, then boot the real service."""
import argparse
from contextlib import closing
import json
import os
from pathlib import Path
import re
import shutil
import sqlite3
import subprocess
import sys
import tempfile

import boot_service
import backup_schedule_cases
import prepare

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'tools'))
import release_candidate as candidate


def game_maintenance_context(root,fixture):
    shutil.copytree(fixture/'game_update',root/'root/bbsbox-ci-game-update',copy_function=os.link)
    for name in ['backup_node.py','update_release.py','rollback_release.py','restore_provisioning.py','install_release.py','install_development.py','enrollment.py','local_tls.py','backup_development.py','maintenance_lock.py']:
        (root/'src/deploy'/name).write_bytes((ROOT/'deploy'/name).read_bytes())
    for name in ['runtime_bundle.py','release_candidate.py','release_materials.py']:
        (root/'src/tools'/name).write_bytes((ROOT/'tools'/name).read_bytes())
    probe=root/'usr/local/lib/bbsbox-game-worker-probe.py'
    probe.parent.mkdir(parents=True,exist_ok=True)
    probe.write_bytes((ROOT/'deploy/ci/game_worker_case.py').read_bytes());probe.chmod(0o644)


def exercise(source,fixture,verifier,root,output,game_node=False):
    if sys.platform!='linux' or os.geteuid()!=0:raise ValueError('Run only on the disposable Linux CI host as root')
    work=(ROOT/'work').resolve()
    source,fixture,root,output=(Path(path).absolute() for path in [source,fixture,root,output])
    if any(path==work or path!=path.resolve() or not path.is_relative_to(work) for path in [source,fixture,root,output]):raise ValueError('Recovery fixture paths must stay inside work/')
    if boot_service.leader(source):raise ValueError('The original fixture must be stopped')
    if root.exists() or output.exists():raise ValueError('Use a fresh replacement fixture')
    output.mkdir(mode=0o755)
    installed=json.loads((source/'etc/bbsbox/installation.json').read_text())
    plans=[]
    for path in (source/'var/lib/bbsbox-maintenance/updates').glob('*/plan.json'):
        plan=json.loads(path.read_text())
        if plan.get('format')=='bbsbox.offline-update-plan/v1' and plan['sequence']==5 and plan['phase']=='completed':plans.append((path.parent/'before.age',plan))
    (archive,plan),=plans
    assert plan['node']==installed['node_id']
    inventory=plan['snapshot']
    if game_node:
        archive=source/'root/bbsbox-ci-game-backup.age'
        evidence=json.loads((source/'root/bbsbox-ci-game-backup-result.json').read_text())
        assert evidence['scope']=='game-node' and evidence['node']==installed['node_id']
        inventory=evidence['inventory_sha256']
    authority=candidate.public_info(fixture/'rotation-public.json')
    recovery_bundle=fixture/('game_recovery' if game_node else 'source_loss_successor')
    release=candidate.command([verifier,'release-inspect','--bundle',recovery_bundle,'--public-key',authority['public_key']])
    expected_migrations=[(item['module'],item['version'],item['sha256']) for item in release['expected_profile']['module_migrations']]
    control_file=source/'var/lib/bbsbox-maintenance/release-control/release-trust.sqlite'
    with closing(sqlite3.connect('file:'+str(control_file)+'?mode=ro',uri=True)) as database:
        control=json.loads(database.execute('SELECT body FROM control WHERE id=1').fetchone()[0])
    assert bytes(control['trusted_key']).hex()==authority['public_key'] and control['highest_sequence']==5 and control['pending'] is None
    original_paths=[archive,source/'var/lib/bbsbox/node.sqlite',source/'etc/bbsbox/tls/ca-key.pem',control_file]
    if game_node:original_paths.append(source/'var/lib/bbsbox-game/state.sqlite')
    original=[candidate.materials.file_digest(path) for path in original_paths]
    with tempfile.TemporaryDirectory(prefix='source-loss-input-',dir=work) as temporary:
        inputs=Path(temporary);media=inputs/'media';media.mkdir(mode=0o700)
        files={'source.age':archive,'operator.key':source/'root/bbsbox-ci-backup.key','bootstrap':verifier.resolve(strict=True),'recovery_guest.py':ROOT/'deploy/ci/recovery_guest.py'}
        for name in ['install_release.py','install_development.py','recover_release.py','restore_provisioning.py','enrollment.py','local_tls.py','backup_development.py']:
            files['deploy/'+name]=ROOT/'deploy'/name
        for name in ['runtime_bundle.py','release_candidate.py','release_materials.py']:
            files['tools/'+name]=ROOT/'tools'/name
        bundle=recovery_bundle
        for path in bundle.rglob('*'):
            if path.is_file():files['bundle/'+path.relative_to(bundle).as_posix()]=path
        for name,path in files.items():
            target=media/name;candidate.copy_file(path,target)
            if name=='bootstrap':target.chmod(0o700)
        (media/'request.json').write_bytes(candidate.materials.encoded(dict(node=plan['node'],inventory_sha256=inventory,public_key=authority['public_key'],minimum_sequence=control['highest_sequence'],game_node=game_node)))
        container=subprocess.check_output(['docker','create','--pull=never','--network=none','--env','SYSTEMD_OFFLINE=1','--env','PYTHONDONTWRITEBYTECODE=1','--mount',f'type=bind,source={media},target=/recovery-inputs,readonly','--entrypoint','/usr/bin/python3','bbsbox-prepared-fixture','/recovery-inputs/recovery_guest.py'],text=True,timeout=30).strip()
        if not re.fullmatch('[0-9a-f]{64}',container):raise ValueError('Unexpected fixture container ID')
        try:
            result=subprocess.run(['docker','start','--attach',container],capture_output=True,timeout=600)
            if result.returncode:raise RuntimeError(result.stderr[-16384:].decode(errors='replace'))
            recovered=json.loads(result.stdout)
            tar=inputs/'replacement.tar'
            subprocess.run(['docker','export',container,'-o',str(tar)],check=True,timeout=120)
        finally:subprocess.run(['docker','rm','-f',container],check=True,capture_output=True,timeout=30)
        prepare.extract(tar,root)
    assert not (root/'recovery-inputs/operator.key').exists() and not (root/'recovery-inputs/source.age').exists()
    assert recovered['installation']['node_id']==installed['node_id'] and recovered['installation']['ca_sha256']==installed['ca_sha256']
    for name in ['ca.pem','ca-key.pem']:assert (root/'etc/bbsbox/tls'/name).read_bytes()==(source/'etc/bbsbox/tls'/name).read_bytes()
    ticket=json.loads((root/'var/lib/bbsbox-maintenance/first-restore.json').read_text())
    assert ticket['source']=='lost' and ticket['minimum_sequence']==5 and ticket['inventory_sha256']==inventory
    assert ticket.get('scope','foundation-node')==('game-node' if game_node else 'foundation-node')
    assert candidate.materials.file_digest(root/'etc/bbsbox/maintenance/recovery.cose')==ticket['decision_sha256']
    with closing(sqlite3.connect('file:'+str(root/'var/lib/bbsbox/node.sqlite')+'?mode=ro',uri=True)) as database:
        applied=database.execute('SELECT module,version,lower(hex(digest)) FROM core_migrations ORDER BY module,version').fetchall()
        assert applied==expected_migrations, 'Restored migrations differ from the verified release profile'
        assert database.execute("SELECT count(*) FROM core_command_receipts WHERE command_id='same-id' AND response=?",(b'kept',)).fetchone()[0]==2
        if not game_node:assert database.execute('SELECT count(*) FROM events_upgrade_receipts').fetchone()[0]==2
        assert database.execute('SELECT revision, background FROM settings_theme WHERE id=1').fetchone()==(1,'#111111')
        assert database.execute('SELECT revision, welcome, menu_order FROM settings_site WHERE id=1').fetchone()==(1,'Welcome neighbors','status,home,account')
    if game_node:
        game=root/'var/lib/bbsbox-game/state.sqlite'
        with closing(sqlite3.connect(game.as_uri()+'?mode=ro',uri=True)) as database:
            assert database.execute('SELECT chamber FROM dungeon').fetchone()==(1,)
            assert database.execute('SELECT count(*) FROM players').fetchone()==(2,)
            assert database.execute('SELECT count(*) FROM receipts').fetchone()==(1,)
        game_maintenance_context(root,fixture)
    def verify_boot(pid,guest,node):
        operator='/opt/bbsbox/maintenance/bbsbox-operator'
        checked=json.loads(boot_service.enter(pid,guest,[operator,'release-control-boot']).stdout)
        state=json.loads(boot_service.enter(pid,guest,[operator,'release-control-state','--directory','/var/lib/bbsbox-maintenance/release-control']).stdout)['state']
        assert checked['expected_node']==node and node==installed['node_id'] and state['highest_sequence']==6 and state['pending'] is None
        assert bytes(state['trusted_key']).hex()==authority['public_key']
        result=dict(independent_release_authority=True,verified_recovered_selection=True,highest_sequence=6)
        if game_node:
            boot_service.enter(pid,guest,['/opt/bbsbox/current/bbsbox-node','backup-keygen','--output','/root/bbsbox-ci-backup.key'])
            result['retained_game_backup']=boot_service.game_backup(pid,guest,node)
            assert boot_service.enter(pid,guest,['/usr/bin/systemctl','is-active','bbsbox-game-worker.socket'],False).stdout.strip()==b'inactive'
            key='/root/bbsbox-ci-backup.key'
            update=json.loads(boot_service.enter(pid,guest,['/usr/bin/python3','/src/deploy/update_release.py','apply','--bundle','/root/bbsbox-ci-game-update','--identity-file',key],timeout=300).stdout)
            assert update['state']=='completed' and update['highest_sequence']==7
            plan=json.loads((guest/'var/lib/bbsbox-maintenance/updates'/update['operation']/'plan.json').read_text())
            assert plan['snapshot_scope']=='game-node'
            socket_unit='bbsbox-game-worker.socket'
            boot_service.enter(pid,guest,['/usr/bin/systemctl','start',socket_unit])
            try:
                boot_service.enter(pid,guest,['/usr/sbin/runuser','-u','bbsbox','--','/usr/bin/python3','/usr/local/lib/bbsbox-game-worker-probe.py','play-more'])
            finally:
                boot_service.enter(pid,guest,['/usr/bin/systemctl','stop',socket_unit])
            rolled=json.loads(boot_service.enter(pid,guest,['/usr/bin/python3','/src/deploy/rollback_release.py','--operation',update['operation'],'--identity-file',key,'--reason','Restore matching game and core snapshot'],timeout=300).stdout)
            assert rolled['state']=='rolled-back' and rolled['highest_sequence']==7
            with closing(sqlite3.connect(game.as_uri()+'?mode=ro',uri=True)) as database:
                assert database.execute('SELECT chamber FROM dungeon').fetchone()==(1,)
            newer='/var/lib/bbsbox-maintenance/updates/'+rolled['operation']+'/displaced-before.age'
            retained='/root/bbsbox-ci-game-update-retained'
            restored=boot_service.enter(pid,guest,['/opt/bbsbox/releases/'+update['release_id']+'/bbsbox-node','restore-game-node','--archive',newer,'--node',node,'--identity-file',key,'--output',retained,'--staging','/var/lib/bbsbox-maintenance'],timeout=120)
            assert json.loads(restored.stdout)['state']=='recovery-required'
            newer_save=guest/retained.lstrip('/')/'games/dungeon-demo/state.sqlite'
            with closing(sqlite3.connect(newer_save.as_uri()+'?mode=ro',uri=True)) as database:
                assert database.execute('SELECT chamber FROM dungeon').fetchone()==(2,)
            result.update(highest_sequence=7,game_update=True,game_rollback=True,newer_game_saved_separately=True)
        else:
            result['scheduled_backup']=backup_schedule_cases.exercise(pid,guest,node)
        return result
    boot_service.boot(root,output/'boot.json',signed=True,maintenance_check=verify_boot)
    assert [candidate.materials.file_digest(path) for path in original_paths]==original and not boot_service.leader(source)
    recovered.update(source_remained_stopped_and_unchanged=True,operator_media_not_exported=True,original_ca_and_node_preserved=True,known_older_registry_migrated=not game_node,placed_game_save_verified=game_node,actual_debian_service_reboot=True)
    (output/'recovery.json').write_text(json.dumps(recovered,indent=2)+'\n');print(json.dumps(recovered,indent=2))


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ['source','fixture','verifier','root','output']:parser.add_argument('--'+name,type=Path,required=True)
    parser.add_argument('--game-node',action='store_true')
    a=parser.parse_args();exercise(a.source,a.fixture,a.verifier,a.root,a.output,a.game_node)


if __name__=='__main__':main()
