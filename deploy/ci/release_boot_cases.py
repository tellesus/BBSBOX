# SPDX-License-Identifier: AGPL-3.0-or-later
"""Additional cases within the verified disposable signed-install guest."""
import json
from contextlib import closing
import os
from pathlib import Path
import re
import sqlite3
import time
import boot_service as guest


def exercise(pid,root):
    installed=json.loads((root/'etc/bbsbox/installation.json').read_text())
    release_id=installed['release_id']
    if not re.fullmatch('[a-f0-9]{64}',release_id):raise ValueError('Unexpected signed fixture release ID')
    operator='/opt/bbsbox/maintenance/bbsbox-operator';control='/var/lib/bbsbox-maintenance/release-control'
    def command(*args):return json.loads(guest.enter(pid,root,[operator,*args],timeout=120).stdout)
    before=command('release-control-state','--directory',control)['state']
    assert before['pending'] is None and before['highest_sequence']==1
    assert bytes(before['active']['release_id']).hex()==release_id
    command('release-control-boot')
    guest.enter(pid,root,['/usr/bin/systemctl','stop','bbsbox.service'])
    source=root/'opt/bbsbox/releases'/release_id
    next_bundle=root/'var/lib/bbsbox-maintenance/ci-next'
    assert not next_bundle.exists() and next_bundle.resolve().is_relative_to(root)
    next_bundle.mkdir(mode=0o700)
    for path in source.rglob('*'):
        if path.is_file() and path.name!='manifest.cose':
            target=next_bundle/path.relative_to(source);target.parent.mkdir(parents=True,exist_ok=True,mode=0o700);os.link(path,target)
    (next_bundle/'manifest.cose').write_bytes((root/'src/next-manifest.cose').read_bytes())
    request='55'*16
    pending=command('release-control-begin','--directory',control,'--revision',str(before['revision']),'--bundle','/var/lib/bbsbox-maintenance/ci-next','--request',request)['state']
    assert pending['highest_sequence']==2 and pending['pending'] is not None
    denied=guest.enter(pid,root,['/usr/bin/systemctl','start','bbsbox.service'],check=False,timeout=30)
    assert denied.returncode!=0
    guest.enter(pid,root,['/usr/bin/systemctl','stop','bbsbox.service'])
    assert guest.enter(pid,root,['/usr/bin/systemctl','is-active','bbsbox.service'],check=False).stdout.strip()!=b'active'
    # No app/data switch occurred in this fence exercise, so retain the exact original selection.
    retained=command('release-control-failed','--directory',control,'--revision',str(pending['revision']),'--request',request,'--reason','CI pending operation stopped before any data or selection change','--at','0')['state']
    assert retained['pending'] is None and retained['highest_sequence']==2
    assert bytes(retained['active']['release_id']).hex()==release_id
    command('release-control-boot')
    assert int(guest.enter(pid,root,['/usr/bin/systemctl','show','bbsbox.service','-p','StartLimitBurst','--value']).stdout)==5
    guest.enter(pid,root,['/usr/bin/systemctl','reset-failed','bbsbox.service']) # Fixture scenario counter only.
    guest.enter(pid,root,['/usr/bin/systemctl','start','bbsbox.service'])
    guest.service_identity(pid,root)
    updates=exercise_updates(pid,root,release_id)
    return dict(root_boot_gate=True,pending_operation_blocks_unit=True,recovery_audited=True,maximum_sequence_preserved=2,active_release_preserved=True,no_release_private_key_on_appliance=True,updates=updates)


def exercise_updates(pid,root,original_release):
    operator='/opt/bbsbox/maintenance/bbsbox-operator';control='/var/lib/bbsbox-maintenance/release-control'
    updater=['/usr/bin/python3','/src/deploy/update_release.py']
    identity=['--identity-file','/root/bbsbox-ci-backup.key']
    def command(*args):return json.loads(guest.enter(pid,root,[operator,*args],timeout=120).stdout)
    def update(name,expected_status=0):
        result=guest.enter(pid,root,[*updater,'apply','--bundle','/var/lib/bbsbox-maintenance/'+name,*identity],check=False,timeout=1200)
        assert result.returncode==expected_status,result.stderr[-8192:].decode(errors='replace')
        return json.loads(result.stdout)
    original_ca=(root/'etc/bbsbox/tls/ca-key.pem').read_bytes()
    node=json.loads((root/'etc/bbsbox/installation.json').read_text())['node_id']
    guest.enter(pid,root,['/usr/bin/systemctl','stop','bbsbox.service'])
    held="import sys,time; from pathlib import Path; sys.path.insert(0,'/src/deploy'); import maintenance_lock\nwith maintenance_lock.acquire():\n Path('/var/lib/bbsbox-maintenance/start-fence-ready').write_text('held'); time.sleep(30)"
    guest.enter(pid,root,['/usr/bin/systemd-run','--unit=bbsbox-maintenance-fence','--property=Type=exec','/usr/bin/python3','-c',held])
    deadline=time.monotonic()+5
    while not (root/'var/lib/bbsbox-maintenance/start-fence-ready').exists():
        if time.monotonic()>deadline:raise ValueError('Maintenance/start fence did not acquire its lock')
        time.sleep(.02)
    assert command('release-control-state','--directory',control)['state']['pending'] is None
    denied=guest.enter(pid,root,['/usr/bin/systemctl','start','bbsbox.service'],check=False,timeout=10)
    assert denied.returncode!=0 # No pending ticket: this denial must come from maintenance exclusion.
    guest.enter(pid,root,['/usr/bin/systemctl','stop','bbsbox-maintenance-fence.service'])
    command('release-control-boot')
    # Seed valid independent idempotency namespaces sharing one command ID.
    guest.enter(pid,root,['/usr/bin/systemctl','stop','bbsbox.service'])
    script = """
import sqlite3
from contextlib import closing

with closing(sqlite3.connect('/var/lib/bbsbox/node.sqlite')) as database, database:
    database.executemany('INSERT INTO core_command_receipts VALUES(?,?,?,?)', [
        (namespace, 'same-id', bytes(32), b'kept') for namespace in ['fixture-one', 'fixture-two']
    ])
    database.execute('INSERT INTO settings_theme VALUES (1, 1, ?, ?, ?, ?)', ('#111111', '#FFFFFF', '#73F7AB', '#FEC566'))
    database.execute('INSERT INTO settings_site VALUES (1, 1, ?, ?, ?)', ('Welcome neighbors', '[-]', 'status,home,account'))
"""
    guest.enter(pid,root,['runuser','-u','bbsbox','--','/usr/bin/python3','-c',script])
    guest.enter(pid,root,['/usr/bin/systemctl','reset-failed','bbsbox.service'])
    guest.enter(pid,root,['/usr/bin/systemctl','start','bbsbox.service'])
    for name in ['upgrade_rejecting','upgrade_interrupted','upgrade_successor']:
        bundle=root/'var/lib/bbsbox-maintenance'/name;bundle.mkdir(mode=0o700)
        for path in (root/'opt/bbsbox/releases'/original_release).rglob('*'):
            if not path.is_file():continue
            relative=path.relative_to(root/'opt/bbsbox/releases'/original_release)
            source=(root/'src'/name/path.name) if relative.as_posix() in ['bbsbox-node','manifest.cose'] else path
            target=bundle/relative;target.parent.mkdir(parents=True,exist_ok=True,mode=0o700);os.link(source,target)
    failed=update('upgrade_rejecting',2)
    assert failed['state']=='recovered' and failed['highest_sequence']==3 and failed['failed_phase']=='migrating'
    after_failure=command('release-control-state','--directory',control)['state']
    assert bytes(after_failure['active']['release_id']).hex()==original_release and after_failure['pending'] is None
    guest.service_identity(pid,root)
    def verify_restored():
        assert (root/'etc/bbsbox/tls/ca-key.pem').read_bytes()==original_ca
        installed=json.loads((root/'etc/bbsbox/installation.json').read_text())
        assert installed['node_id']==node and installed['release_id']==original_release
        with closing(sqlite3.connect('file:'+str(root/'var/lib/bbsbox/node.sqlite')+'?mode=ro',uri=True)) as database:
            assert database.execute('SELECT count(*) FROM core_migrations').fetchone()[0]==16
            assert database.execute("SELECT count(*) FROM core_command_receipts WHERE command_id='same-id' AND response=?",(b'kept',)).fetchone()[0]==2
            assert database.execute('SELECT revision, background FROM settings_theme WHERE id=1').fetchone()==(1,'#111111')
            assert database.execute('SELECT revision, welcome, menu_order FROM settings_site WHERE id=1').fetchone()==(1,'Welcome neighbors','status,home,account')
        assert (root/'etc/bbsbox/maintenance/recovery.cose').is_file()
    verify_restored()
    # A real systemd operation/child group dies during a valid, expensive compiled SQL migration.
    guest.enter(pid,root,['/usr/bin/systemd-run','--unit=bbsbox-update-fixture','--property=Type=exec','--property=KillMode=control-group',*updater,'apply','--bundle','/var/lib/bbsbox-maintenance/upgrade_interrupted',*identity])
    deadline=time.monotonic()+180;operation=None
    while time.monotonic()<deadline:
        plans=list((root/'var/lib/bbsbox-maintenance/updates').glob('*/plan.json'))
        for path in plans:
            plan=json.loads(path.read_text())
            if plan['sequence']==4 and plan['phase']=='migrating':operation=plan['request']
        if operation:
            for process in Path('/proc').iterdir():
                if not process.name.isdecimal():continue
                try:
                    if not guest.same_root(int(process.name),root) or 'bbsbox-update-fixture.service' not in (process/'cgroup').read_text():continue
                    arguments=(process/'cmdline').read_bytes().split(b'\0')
                    ticks=(process/'stat').read_text().rsplit(')',1)[1].split()
                    if b'migrate-existing' in arguments and int(ticks[11])+int(ticks[12])>=5:break
                except (OSError,ValueError):continue
            else:time.sleep(.02);continue
            break
        time.sleep(.02)
    else:raise ValueError('Did not observe the actual expensive migration child before its deadline')
    config_before=(root/'etc/bbsbox/node.json').read_bytes()
    renewal=guest.enter(pid,root,['/usr/bin/python3','/src/deploy/renew_tls.py','--development'],check=False,timeout=30)
    assert renewal.returncode!=0 and (root/'etc/bbsbox/node.json').read_bytes()==config_before
    guest.enter(pid,root,['/usr/bin/systemctl','kill','--kill-whom=all','--signal=KILL','bbsbox-update-fixture.service'])
    guest.enter(pid,root,['/usr/bin/systemctl','stop','bbsbox-update-fixture.service'])
    pending=command('release-control-state','--directory',control)['state']
    assert pending['highest_sequence']==4 and bytes(pending['pending']['request']).hex()==operation
    denied=guest.enter(pid,root,['/usr/bin/systemctl','start','bbsbox.service'],check=False,timeout=30)
    assert denied.returncode!=0
    guest.enter(pid,root,['/usr/bin/systemctl','reset-failed','bbsbox.service'])
    restored=json.loads(guest.enter(pid,root,[*updater,'recover','--operation',operation,*identity],timeout=1200).stdout)
    assert restored['state']=='recovered' and restored['highest_sequence']==4
    verify_restored();guest.service_identity(pid,root)
    succeeded=update('upgrade_successor')
    assert succeeded['state']=='completed' and succeeded['highest_sequence']==5
    current=command('release-control-state','--directory',control)['state']
    assert current['pending'] is None and current['highest_sequence']==5 and bytes(current['active']['release_id']).hex()==succeeded['release_id']
    rotated=json.loads((root/'src/rotation-public.json').read_text())
    assert current['trusted_key']!=after_failure['trusted_key'] and bytes(current['trusted_key']).hex()==rotated['public_key']
    assert json.loads((root/'etc/bbsbox/installation.json').read_text())['node_id']==node
    assert (root/'etc/bbsbox/tls/ca-key.pem').read_bytes()==original_ca
    with closing(sqlite3.connect('file:'+str(root/'var/lib/bbsbox/node.sqlite')+'?mode=ro',uri=True)) as database:
        assert database.execute('SELECT count(*) FROM core_migrations').fetchone()[0]==18
        assert database.execute('SELECT count(*) FROM events_upgrade_receipts').fetchone()[0]==2
    command('release-control-boot');guest.service_identity(pid,root)
    # Completion retry must preserve a write accepted after that update's snapshot/completion.
    guest.enter(pid,root,['/usr/bin/systemctl','stop','bbsbox.service'])
    later="import sqlite3; c=sqlite3.connect('/var/lib/bbsbox/node.sqlite'); c.execute('INSERT INTO core_command_receipts VALUES(?,?,?,?)',('fixture-later','after-update',bytes(32),b'newer-write')); c.commit(); c.close()"
    guest.enter(pid,root,['runuser','-u','bbsbox','--','/usr/bin/python3','-c',later])
    repeated=json.loads(guest.enter(pid,root,[*updater,'recover','--operation',succeeded['operation'],*identity],timeout=120).stdout)
    assert repeated['state']=='completed'
    with closing(sqlite3.connect('file:'+str(root/'var/lib/bbsbox/node.sqlite')+'?mode=ro',uri=True)) as database:
        assert database.execute("SELECT response FROM core_command_receipts WHERE namespace='fixture-later'").fetchone()[0]==b'newer-write'
        assert database.execute('SELECT count(*) FROM core_migrations').fetchone()[0]==18
    stale=guest.enter(pid,root,[*updater,'recover','--operation',failed['operation'],*identity],check=False,timeout=120)
    assert stale.returncode!=0
    assert command('release-control-state','--directory',control)['state']==current
    # The upgraded assembly must also back up its new exact registry, preserving an inactive source.
    guest.enter(pid,root,['/usr/bin/systemctl','stop','bbsbox.service'])
    recipient=json.loads(guest.enter(pid,root,['/opt/bbsbox/current/bbsbox-node','backup-public',*identity]).stdout)['recipient']
    backed=json.loads(guest.enter(pid,root,['/usr/bin/python3','/src/deploy/backup_node.py','--output','/root/upgraded.age','--recipient',recipient],timeout=120).stdout)
    assert backed['service_resumed'] is False and backed['node']==node
    guest.enter(pid,root,['/usr/bin/systemctl','reset-failed','bbsbox.service'])
    guest.enter(pid,root,['/usr/bin/systemctl','start','bbsbox.service']);guest.service_identity(pid,root)
    rollback=json.loads(guest.enter(pid,root,['/usr/bin/python3','/src/deploy/rollback_release.py','--operation',succeeded['operation'],*identity,'--reason','CI explicit matching historical recovery'],timeout=1200).stdout)
    assert rollback['state']=='rolled-back' and rollback['highest_sequence']==5
    verify_restored();guest.service_identity(pid,root)
    after_rollback=command('release-control-state','--directory',control)['state']
    assert after_rollback['pending'] is None and after_rollback['highest_sequence']==5
    assert bytes(after_rollback['active']['release_id']).hex()==original_release and after_rollback['trusted_key']==current['trusted_key']
    with closing(sqlite3.connect('file:'+str(root/'var/lib/bbsbox/node.sqlite')+'?mode=ro',uri=True)) as database:
        assert database.execute("SELECT count(*) FROM core_command_receipts WHERE namespace='fixture-later'").fetchone()[0]==0
    plan=json.loads((root/'var/lib/bbsbox-maintenance/updates'/rollback['operation']/'plan.json').read_text())
    retained=json.loads(guest.enter(pid,root,['/opt/bbsbox/releases/'+succeeded['release_id']+'/bbsbox-node','restore','--archive','/var/lib/bbsbox-maintenance/updates/'+rollback['operation']+'/displaced-before.age','--node',node,*identity,'--output','/root/rollback-retained','--staging','/var/lib/bbsbox-maintenance'],timeout=120).stdout)
    assert retained['inventory_sha256']==plan['retained_inventory'] and retained['state']=='recovery-required'
    with closing(sqlite3.connect('file:'+str(root/'root/rollback-retained/data/node.sqlite')+'?mode=ro',uri=True)) as database:
        assert database.execute("SELECT response FROM core_command_receipts WHERE namespace='fixture-later'").fetchone()[0]==b'newer-write'
        assert database.execute('SELECT count(*) FROM core_migrations').fetchone()[0]==18
    with closing(sqlite3.connect('file:'+str(root/'var/lib/bbsbox-maintenance/release-control/release-trust.sqlite')+'?mode=ro',uri=True)) as database:
        evidence=json.loads(database.execute("SELECT detail FROM history WHERE action='begin-recovery' ORDER BY revision DESC LIMIT 1").fetchone()[0])['purpose']['evidence']
        assert evidence['reason']=='CI explicit matching historical recovery' and bytes(evidence['snapshot_inventory']).hex()==plan['snapshot']
    command('release-control-boot')
    return dict(manual_start_excluded_before_pending=True,failed_sql_restored_matching_snapshot=True,actual_migration_process_killed=True,renewal_excluded_during_update=True,pending_interruption_blocks_boot=True,explicit_interrupted_recovery=True,new_sql_migrated=True,receipts_identity_ca_preserved=True,completed_retry_keeps_newer_writes=True,stale_operation_refused=True,upgraded_registry_backup=True,completed_rollback_recorded=True,displaced_newer_writes_retained_separately=True,prior_key_rotation_preserved=True,maximum_sequence_preserved=5)
