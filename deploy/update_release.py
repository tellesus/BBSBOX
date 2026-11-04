# SPDX-License-Identifier: AGPL-3.0-or-later
"""Offline foundation-node updates and matching pre-update recovery on the signed Debian layout."""
import argparse
from contextlib import contextmanager
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import tempfile
import time

import install_release as install
import maintenance_lock

candidate=install.candidate
materials=candidate.materials
APPLICATION=Path('/opt/bbsbox')
MAINTENANCE=Path('/var/lib/bbsbox-maintenance')
CONTROL=MAINTENANCE/'release-control'
OPERATOR=APPLICATION/'maintenance/bbsbox-operator'
CONFIGURATION=Path('/etc/bbsbox')
DATA=Path('/var/lib/bbsbox')
UNIT=Path('/etc/systemd/system/bbsbox.service')
DATA_NAMES={'node.sqlite','node.sqlite-wal','node.sqlite-shm','node.sqlite-journal','node.sqlite.writer.lock','node.sqlite.blobs'}


def sync(directory):
    descriptor=os.open(directory,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
    try:os.fsync(descriptor)
    finally:os.close(descriptor)


def protected(path,directory=False):
    if path.absolute()!=path.resolve():raise ValueError('Maintenance paths must not traverse links')
    metadata=path.lstat()
    if metadata.st_uid!=0 or metadata.st_mode&0o022 or (not stat.S_ISDIR(metadata.st_mode) if directory else not stat.S_ISREG(metadata.st_mode)):
        raise ValueError('Expected a protected root-owned maintenance path')


def atomic(path,raw,mode=0o600,gid=0):
    if path.exists():
        metadata=path.lstat()
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid!=0:raise ValueError('Refusing an unexpected replacement target')
    descriptor,name=tempfile.mkstemp(prefix='.update-',dir=path.parent)
    try:
        with os.fdopen(descriptor,'wb') as stream:
            stream.write(raw);stream.flush();os.fchown(stream.fileno(),0,gid);os.fchmod(stream.fileno(),mode);os.fsync(stream.fileno())
        os.replace(name,path);sync(path.parent)
    finally:
        if os.path.exists(name):os.unlink(name)


def state(action='state',*arguments):
    return install.control(OPERATOR,CONTROL,action,*arguments)


def installed_release(release,authority,expected):
    for path in [release,*release.rglob('*')]:protected(path,path.is_dir())
    report=command(OPERATOR,'release-inspect','--bundle',release,'--public-key',authority)
    if report.get('release_id')!=expected:raise ValueError('Installed release bytes differ from the selected receipt')
    return report


def command(*arguments,timeout=1200):
    return materials.decoded(candidate.bounded_run(arguments,timeout=timeout))


def service(action,check=True):
    result=subprocess.run(['/usr/bin/systemctl',action,'bbsbox.service'],capture_output=True,timeout=40)
    if check and result.returncode:raise ValueError('Node service '+action+' failed')
    return result.stdout.strip()


def compatible(previous,successor):
    if previous.get('target')!='debian13-amd64' or successor.get('target')!=previous['target'] or previous.get('core_schema')!=successor.get('core_schema') or previous['core_schema'] not in successor.get('readable_core_schemas',[]):
        raise ValueError('This update provider requires the same directly readable core schema')
    old={(m['module'],m['version']):m['sha256'] for m in previous['module_migrations']}
    new={(m['module'],m['version']):m['sha256'] for m in successor['module_migrations']}
    if any(owner not in ['identity', 'events', 'settings', 'discussions', 'mail', 'profiles', 'social'] for owner, _ in new) or any(new.get(key)!=digest for key,digest in old.items()):
        raise ValueError('Update removed/changed applied SQL or needs another module snapshot provider')


def durable_tree(root,uid=0,gid=0,public=False):
    directories=[]
    for path in [root,*root.rglob('*')]:
        metadata=path.lstat()
        if stat.S_ISDIR(metadata.st_mode):
            path.chmod(0o755 if public else 0o700);os.chown(path,uid,gid);directories.append(path)
        elif stat.S_ISREG(metadata.st_mode):
            descriptor=os.open(path,os.O_RDONLY|os.O_NOFOLLOW)
            try:
                os.fchown(descriptor,uid,gid);os.fchmod(descriptor,(0o755 if path.name=='bbsbox-node' else 0o644) if public else 0o600);os.fsync(descriptor)
            finally:os.close(descriptor)
        else:raise ValueError('Unexpected linked/special maintenance entry')
    for directory in sorted(directories,key=lambda p:len(p.parts),reverse=True):sync(directory)


def choose(release):
    protected(release,True)
    atomic(UNIT,(release/'bbsbox.service').read_bytes(),0o644)
    temporary=APPLICATION/('.current-'+os.urandom(16).hex())
    temporary.symlink_to(release,target_is_directory=True)
    os.replace(temporary,APPLICATION/'current');sync(APPLICATION)
    subprocess.run(['/usr/bin/systemctl','daemon-reload'],check=True,capture_output=True,timeout=40)


def save(directory,plan,phase,**updates):
    plan.update(updates,phase=phase)
    atomic(directory/'plan.json',materials.encoded(plan))


def prerequisites(identity):
    install.base.platform_preflight()
    for directory in [APPLICATION,APPLICATION/'releases',MAINTENANCE,CONTROL,CONFIGURATION]:protected(directory,True)
    protected(OPERATOR)
    import pwd
    user=pwd.getpwnam('bbsbox');metadata=DATA.lstat()
    if DATA!=DATA.resolve() or not stat.S_ISDIR(metadata.st_mode) or metadata.st_uid!=user.pw_uid or metadata.st_mode&0o077 or not user.pw_uid or not user.pw_gid:
        raise ValueError('Expected private storage owned by the unprivileged node account')
    identity=Path(identity).absolute()
    protected(identity)
    if identity.stat().st_mode&0o077 or any(identity.is_relative_to(p) for p in [DATA,CONFIGURATION,APPLICATION]):
        raise ValueError('Use a separate private operator backup identity')
    return identity


def finish_service(plan):
    installed=install.enrollment.read_json(CONFIGURATION/'installation.json')
    if installed.get('format')!='bbsbox.install/v1' or installed.get('node_id')!=plan['node']:raise ValueError('Installed identity differs from the operation')
    command(OPERATOR,'release-control-installed',timeout=20)
    import pwd
    user=pwd.getpwnam('bbsbox')
    if not user.pw_uid or not user.pw_gid:raise ValueError('Expected the unprivileged node account')
    install.backup.repair_ownership(DATA/'node.sqlite',user.pw_uid,user.pw_gid)


def resume_service(plan):
    if plan['service_was_active']:service('start')


@contextmanager
def source_lock():
    # Same Linux flock as M02's writer. A surviving migration child must exit before replacement.
    import fcntl
    descriptor=os.open(DATA/'node.sqlite.writer.lock',os.O_RDWR|os.O_NOFOLLOW)
    try:
        if not stat.S_ISREG(os.fstat(descriptor).st_mode):raise ValueError('Expected the existing writer lock')
        fcntl.flock(descriptor,fcntl.LOCK_EX|fcntl.LOCK_NB)
        yield
    finally:os.close(descriptor)


def configuration_matches(saved):
    """This first updater changes installation metadata only, never TLS/address configuration."""
    def files(root):
        result={}
        for path in root.rglob('*'):
            metadata=path.lstat()
            if stat.S_ISDIR(metadata.st_mode):continue
            name=path.relative_to(root).as_posix()
            if name in ['.tls-renewal.lock','installation.json','maintenance/recovery.cose']:continue
            if not stat.S_ISREG(metadata.st_mode):raise ValueError('Unexpected configuration entry')
            result[name]=materials.file_digest(path)
        return result
    if files(saved)!=files(CONFIGURATION):raise ValueError('Configuration changed outside the update; reconcile before recovery')


def recover_locked(directory,plan,identity):
    current=state()
    pending=current['pending']
    rollback=plan['format']=='bbsbox.offline-rollback-plan/v1'
    if pending is None:
        selected=bytes(current['active']['release_id']).hex() if current['active'] else None
        if rollback:
            if selected==plan['previous'] and plan['phase'] in ['rollback-durable','rolled-back'] and current['revision']==plan['pending_revision']+1:
                save(directory,plan,'rolled-back')
                return dict(format='bbsbox.offline-update/v1',operation=plan['request'],state='rolled-back',node=plan['node'],highest_sequence=current['highest_sequence'])
            if selected==plan['candidate'] and current['revision']==plan['begin_revision']:
                save(directory,plan,'cancelled')
                return dict(format='bbsbox.offline-update/v1',operation=plan['request'],state='cancelled',node=plan['node'])
            raise ValueError('Recorded rollback no longer matches release control')
        if selected==plan['candidate']:
            save(directory,plan,'completed')
            return dict(format='bbsbox.offline-update/v1',operation=plan['request'],state='completed',node=plan['node'])
        if (selected==plan['previous'] and current['revision']==plan['begin_revision']) or (selected==plan['previous'] and plan['phase']=='rollback-durable' and current['revision']==plan['pending_revision']+1):
            save(directory,plan,'recovered')
            return dict(format='bbsbox.offline-update/v1',operation=plan['request'],state='recovered',node=plan['node'])
        if plan['phase']=='recovered' and selected==plan['previous']:
            return dict(format='bbsbox.offline-update/v1',operation=plan['request'],state='recovered',node=plan['node'])
        raise ValueError('Operation no longer matches release control')
    target=plan['previous'] if rollback else plan['candidate']
    source=plan['candidate'] if rollback else plan['previous']
    if bytes(pending['request']).hex()!=plan['request'] or bytes(pending['candidate']['release_id']).hex()!=target or pending['purpose']['kind']!=('recovery' if rollback else 'install') or not current['active'] or bytes(current['active']['release_id']).hex()!=source:
        raise ValueError('Recovery ticket differs from current pending operation')
    if rollback and bytes(pending['purpose']['evidence']['snapshot_inventory']).hex()!=plan['snapshot']:raise ValueError('Recorded rollback snapshot differs from its control evidence')
    prior=pending['candidate'] if rollback else current['active']
    if bytes(prior['authority']).hex()!=plan['previous_authority']:raise ValueError('Historical release authority differs from its receipt')
    service('stop')
    with source_lock():result=restore_pending(directory,plan,identity,current)
    return result


def restore_pending(directory,plan,identity,current):
    previous=APPLICATION/'releases'/plan['previous']
    # Check the old receipt/binary using its saved authority, independently of a newer pending key.
    installed_release(previous,plan['previous_authority'],plan['previous'])
    work=DATA/'.maintenance';install.backup.prepare_stage(work)
    with tempfile.TemporaryDirectory(prefix='restore-',dir=work) as temporary:
        restored=Path(temporary)/'restored'
        reason=('Recorded rollback; stopped source: '+plan['reason']) if plan['format']=='bbsbox.offline-rollback-plan/v1' else 'Failed or interrupted update; source service stopped on this appliance'
        evidence=command(previous/'bbsbox-node','restore-and-authorize','--archive',directory/'before.age','--node',plan['node'],'--identity-file',identity,'--output',restored,'--staging',MAINTENANCE,'--source','retired','--reason',reason,'--at',str(int(time.time())))
        if evidence.get('inventory_sha256')!=plan['snapshot'] or evidence.get('isolated_writer_ready') is not True or evidence.get('installed') is not False:raise ValueError('Recovery does not match the pre-update snapshot')
        configuration_matches(restored/'configuration')
        installed=install.enrollment.read_json(restored/'configuration/installation.json')
        if installed.get('node_id')!=plan['node'] or installed.get('release_id')!=plan['previous']:raise ValueError('Snapshot does not belong to the previous selection')
        present=install.enrollment.read_json(CONFIGURATION/'installation.json')
        if present.get('format')!='bbsbox.install/v1' or present.get('node_id')!=plan['node'] or present.get('release_id') not in [plan['previous'],plan['candidate']]:raise ValueError('Current installation metadata changed outside this update')
        # Only known core entries move; external control and unknown data are never overwritten.
        for path in DATA.iterdir():
            if path.name=='.maintenance':continue
            if path.name not in DATA_NAMES or path.is_symlink():raise ValueError('Unexpected node storage entry; preserve it for reconciliation')
        import pwd
        user=pwd.getpwnam('bbsbox')
        if not user.pw_uid or not user.pw_gid:raise ValueError('Expected the unprivileged node account')
        durable_tree(restored/'data',user.pw_uid,user.pw_gid)
        displaced=work/('displaced-'+os.urandom(16).hex());displaced.mkdir(mode=0o700);sync(work)
        save(directory,plan,'restoring',pending_revision=current['revision'])
        for name in sorted(DATA_NAMES):
            if name=='node.sqlite.writer.lock':continue # Retain its inode and exclusion across retries.
            original=DATA/name
            if original.exists():original.rename(displaced/name);sync(DATA);sync(displaced)
        for path in (restored/'data').iterdir():
            if path.name not in DATA_NAMES:raise ValueError('Unexpected restored core entry')
            if path.name=='node.sqlite.writer.lock':continue
            path.rename(DATA/path.name);sync(restored/'data');sync(DATA)
        atomic(CONFIGURATION/'installation.json',materials.encoded(installed),0o640,user.pw_gid)
        recovery_directory=CONFIGURATION/'maintenance'
        if not recovery_directory.exists():recovery_directory.mkdir(mode=0o700);sync(CONFIGURATION)
        protected(recovery_directory,True)
        atomic(recovery_directory/'recovery.cose',(restored/'configuration/maintenance/recovery.cose').read_bytes())
        choose(previous)
        save(directory,plan,'rollback-durable',pending_revision=current['revision'])
        if plan['format']=='bbsbox.offline-rollback-plan/v1':
            recovered=state('complete','--revision',str(current['revision']),'--request',plan['request'],'--release-id',plan['previous']);outcome='rolled-back'
        else:
            recovered=state('failed','--revision',str(current['revision']),'--request',plan['request'],'--reason','Restored the matching pre-update snapshot and prior application','--at',str(int(time.time())));outcome='recovered'
        save(directory,plan,outcome,final_revision=recovered['revision'])
    return dict(format='bbsbox.offline-update/v1',operation=plan['request'],state=outcome,node=plan['node'],highest_sequence=recovered['highest_sequence'],post_snapshot_writes='not-reconciled',client_recovery='separate')


def apply(bundle,identity):
    identity=prerequisites(identity)
    with maintenance_lock.acquire():
        result=apply_locked(bundle,identity)
        plan=completed_plan(result)
    resume_service(plan)
    return result


def completed_plan(result):
    operation=result['operation']
    plan=materials.decoded(materials.read_file(MAINTENANCE/'updates'/operation/'plan.json',65536))
    validate_plan(plan,operation)
    finish_service(plan)
    return plan


def apply_locked(bundle,identity):
    command(OPERATOR,'release-control-installed',timeout=20)
    before=state();previous=bytes(before['active']['release_id']).hex()
    previous_release=APPLICATION/'releases'/previous
    installed=install.enrollment.read_json(CONFIGURATION/'installation.json')
    config=install.enrollment.read_json(CONFIGURATION/'node.json')
    if installed.get('format')!='bbsbox.install/v1' or installed.get('release_id')!=previous or config.get('database')!=str(DATA/'node.sqlite'):
        raise ValueError('Expected the current signed foundation-node installation')
    recipient=command(previous_release/'bbsbox-node','backup-public','--identity-file',identity,timeout=20)['recipient']
    with tempfile.TemporaryDirectory(prefix='.update-',dir=APPLICATION/'releases') as temporary:
        staged=Path(temporary)/'verified'
        verified=candidate.inspect(OPERATOR,Path(bundle),bytes(before['trusted_key']).hex(),staged,minimum_sequence=before['highest_sequence']+1)
        compatible(command(previous_release/'bbsbox-node','--release-profile',timeout=20),command(staged/'bbsbox-node','--release-profile',timeout=20))
        release=APPLICATION/'releases'/verified['release_id']
        if release.exists() or release.is_symlink():
            # A preparation crash may have published this exact verified release before its plan.
            installed_release(release,bytes(before['trusted_key']).hex(),verified['release_id'])
        else:
            durable_tree(staged,public=True);staged.rename(release);sync(release.parent)
    updates=MAINTENANCE/'updates';install.backup.prepare_stage(updates)
    request=os.urandom(16).hex();directory=updates/request;directory.mkdir(mode=0o700);sync(updates)
    active=service('is-active',False)
    if active not in [b'active',b'inactive',b'failed']:raise ValueError('Wait for the service state transition')
    was_active=active==b'active'
    plan=dict(format='bbsbox.offline-update-plan/v1',request=request,node=installed['node_id'],previous=previous,previous_authority=bytes(before['active']['authority']).hex(),candidate=verified['release_id'],sequence=verified['sequence'],snapshot=None,begin_revision=before['revision'],service_was_active=was_active)
    save(directory,plan,'preparing')
    try:
        service('stop')
        snapshot=command(previous_release/'bbsbox-node','backup','--config',CONFIGURATION/'node.json','--node',installed['node_id'],'--recipient',recipient,'--output',directory/'before.age','--staging',MAINTENANCE)
        if snapshot.get('node')!=installed['node_id'] or snapshot.get('format')!='bbsbox.node-backup/v1':raise ValueError('Unexpected pre-update snapshot')
        sync(directory)
        # Prove this exact encrypted snapshot can be opened with the supplied identity first.
        with tempfile.TemporaryDirectory(prefix='pre-update-',dir=MAINTENANCE) as temporary:
            checked=command(previous_release/'bbsbox-node','restore','--archive',directory/'before.age','--node',installed['node_id'],'--identity-file',identity,'--output',Path(temporary)/'checked','--staging',MAINTENANCE)
            if checked.get('inventory_sha256')!=snapshot['inventory_sha256'] or checked.get('state')!='recovery-required':raise ValueError('Pre-update snapshot could not be verified')
        save(directory,plan,'snapshot-ready',snapshot=snapshot['inventory_sha256'])
        pending=state('begin','--revision',str(before['revision']),'--bundle',release,'--request',request)
        save(directory,plan,'migrating',pending_revision=pending['revision'])
        migrated=command(release/'bbsbox-node','migrate-existing','--database',DATA/'node.sqlite','--node',installed['node_id'])
        if migrated.get('format')!='bbsbox.migration-existing/v1' or migrated.get('node')!=installed['node_id'] or migrated.get('listener_opened') is not False:raise ValueError('Unexpected migration response')
        import pwd
        user=pwd.getpwnam('bbsbox');install.backup.repair_ownership(DATA/'node.sqlite',user.pw_uid,user.pw_gid)
        installed.update(release_id=verified['release_id'],release_sequence=verified['sequence'],source_commit=verified['source_commit'])
        atomic(CONFIGURATION/'installation.json',materials.encoded(installed),0o640,user.pw_gid)
        choose(release)
        save(directory,plan,'selection-durable')
        completed=state('complete','--revision',str(pending['revision']),'--request',request,'--release-id',verified['release_id'])
        save(directory,plan,'completed',final_revision=completed['revision'])
    except Exception as error:
        failed_phase=plan['phase']
        try:
            recovered=recover_locked(directory,plan,identity)
            recovered.update(failed_phase=failed_phase,failure_type=type(error).__name__)
            return recovered
        except Exception as recovery_error:raise ValueError('Update '+request+' requires explicit recovery; its control ticket remains authoritative') from recovery_error
    return dict(format='bbsbox.offline-update/v1',operation=request,state='completed',node=installed['node_id'],release_id=verified['release_id'],highest_sequence=completed['highest_sequence'])


def recover(operation,identity):
    identity=prerequisites(identity)
    if not re.fullmatch('[0-9a-f]{32}',operation):raise ValueError('Invalid update operation identifier')
    with maintenance_lock.acquire():
        directory=MAINTENANCE/'updates'/operation;protected(directory,True);protected(directory/'plan.json')
        plan=materials.decoded(materials.read_file(directory/'plan.json',65536))
        validate_plan(plan,operation)
        result=recover_locked(directory,plan,identity)
        finish_service(plan)
    resume_service(plan)
    return result


def validate_plan(plan,operation):
    if not isinstance(plan,dict) or plan.get('format') not in ['bbsbox.offline-update-plan/v1','bbsbox.offline-rollback-plan/v1'] or plan.get('request')!=operation or not re.fullmatch('[0-9a-f]{32}',operation):raise ValueError('Invalid update recovery plan')
    if any(not isinstance(plan.get(name),str) or not re.fullmatch('[0-9a-f]{64}',plan[name]) for name in ['previous','candidate','previous_authority']):raise ValueError('Invalid release binding in recovery plan')
    if not isinstance(plan.get('node'),str) or not re.fullmatch('node:[0-9a-f]{64}',plan['node']) or type(plan.get('service_was_active')) is not bool:raise ValueError('Invalid source binding in recovery plan')
    if type(plan.get('begin_revision')) is not int or not 0<=plan['begin_revision']<2**63 or type(plan.get('sequence')) is not int or not 1<=plan['sequence']<2**64:raise ValueError('Invalid recovery plan fence')
    if plan.get('snapshot') is not None and (not isinstance(plan['snapshot'],str) or not re.fullmatch('[0-9a-f]{64}',plan['snapshot'])):raise ValueError('Invalid snapshot binding in recovery plan')
    if plan.get('phase') not in ['preparing','snapshot-ready','migrating','selection-durable','completed','restoring','rollback-durable','recovered','rolled-back','cancelled']:raise ValueError('Unknown recovery plan phase')
    if 'pending_revision' in plan and (type(plan['pending_revision']) is not int or plan['pending_revision']!=plan['begin_revision']+1):raise ValueError('Invalid pending revision in recovery plan')
    if plan['format']=='bbsbox.offline-rollback-plan/v1':
        reason=plan.get('reason')
        if not isinstance(reason,str) or not reason.strip() or len(reason.encode())>360 or any(ord(c)<32 or 127<=ord(c)<=159 for c in reason) or plan['snapshot'] is None:raise ValueError('Recorded rollback needs its reason and exact snapshot')


def main():
    parser=argparse.ArgumentParser(description=__doc__);commands=parser.add_subparsers(dest='command',required=True)
    apply_parser=commands.add_parser('apply');apply_parser.add_argument('--bundle',type=Path,required=True);apply_parser.add_argument('--identity-file',type=Path,required=True)
    recovery_parser=commands.add_parser('recover');recovery_parser.add_argument('--operation',required=True);recovery_parser.add_argument('--identity-file',type=Path,required=True)
    args=parser.parse_args()
    result=apply(args.bundle,args.identity_file) if args.command=='apply' else recover(args.operation,args.identity_file)
    print(json.dumps(result,indent=2))
    if args.command=='apply' and result['state']!='completed':return 2


if __name__=='__main__':raise SystemExit(main())
