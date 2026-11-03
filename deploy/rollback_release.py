# SPDX-License-Identifier: AGPL-3.0-or-later
"""Explicit recovery of a previously completed release with its matching pre-update snapshot."""
import argparse
import json
import os
from pathlib import Path
import re
import tempfile
import time

import update_release as update


def rollback(operation,identity,reason):
    identity=update.prerequisites(identity)
    if not re.fullmatch('[0-9a-f]{32}',operation):raise ValueError('Invalid source update operation')
    if not reason.strip() or len(reason.encode())>360 or any(ord(c)<32 or 127<=ord(c)<=159 for c in reason):raise ValueError('Supply a reason up to 360 UTF-8 bytes without controls')
    with update.maintenance_lock.acquire():
        result=rollback_locked(operation,identity,reason)
        plan=update.completed_plan(result)
    update.resume_service(plan)
    return result


def rollback_locked(operation,identity,reason):
    update.command(update.OPERATOR,'release-control-installed',timeout=20)
    source=update.MAINTENANCE/'updates'/operation
    update.protected(source,True);update.protected(source/'plan.json');update.protected(source/'before.age')
    original=update.materials.decoded(update.materials.read_file(source/'plan.json',65536));update.validate_plan(original,operation)
    current=update.state()
    selected=bytes(current['active']['release_id']).hex()
    if original['format']!='bbsbox.offline-update-plan/v1' or original['phase']!='completed' or selected!=original['candidate']:raise ValueError('Rollback requires the matching completed update still selected')
    installed=update.install.enrollment.read_json(update.CONFIGURATION/'installation.json')
    if installed.get('format')!='bbsbox.install/v1' or installed.get('node_id')!=original['node'] or installed.get('release_id')!=selected:raise ValueError('Source installation does not match the completed update')
    executable=update.APPLICATION/'releases'/selected/'bbsbox-node'
    previous=update.APPLICATION/'releases'/original['previous']
    update.installed_release(previous,original['previous_authority'],original['previous'])
    recipient=update.command(executable,'backup-public','--identity-file',identity,timeout=20)['recipient']
    # Validate the selected historical snapshot/key/registry before stopping a healthy current node.
    with tempfile.TemporaryDirectory(prefix='rollback-check-',dir=update.MAINTENANCE) as temporary:
        checked=update.command(previous/'bbsbox-node','restore','--archive',source/'before.age','--node',original['node'],'--identity-file',identity,'--output',Path(temporary)/'checked','--staging',update.MAINTENANCE)
        if checked.get('inventory_sha256')!=original['snapshot']:raise ValueError('Historical snapshot differs from the completed update plan')
        update.configuration_matches(Path(temporary)/'checked/configuration')
    active=update.service('is-active',False)
    if active not in [b'active',b'inactive',b'failed']:raise ValueError('Wait for the source service state transition')
    request=os.urandom(16).hex();directory=update.MAINTENANCE/'updates'/request
    directory.mkdir(mode=0o700);update.sync(directory.parent)
    # Immutable encrypted input remains separate from both live node data and release control.
    os.link(source/'before.age',directory/'before.age');update.sync(directory)
    plan=dict(format='bbsbox.offline-rollback-plan/v1',request=request,node=original['node'],previous=original['previous'],previous_authority=original['previous_authority'],candidate=selected,sequence=current['highest_sequence'],snapshot=original['snapshot'],begin_revision=current['revision'],service_was_active=active==b'active',reason=reason,source_operation=operation)
    update.save(directory,plan,'preparing')
    try:
        update.service('stop')
        # Retain accepted post-update data for separate reconciliation; never claim it was merged.
        retained=update.command(executable,'backup','--config',update.CONFIGURATION/'node.json','--node',plan['node'],'--recipient',recipient,'--output',directory/'displaced-before.age','--staging',update.MAINTENANCE)
        if retained.get('node')!=plan['node'] or retained.get('format')!='bbsbox.node-backup/v1':raise ValueError('Could not retain the current node snapshot')
        update.sync(directory)
        with tempfile.TemporaryDirectory(prefix='rollback-current-',dir=update.MAINTENANCE) as temporary:
            checked=update.command(executable,'restore','--archive',directory/'displaced-before.age','--node',plan['node'],'--identity-file',identity,'--output',Path(temporary)/'checked','--staging',update.MAINTENANCE)
            if checked.get('inventory_sha256')!=retained['inventory_sha256']:raise ValueError('Current retained snapshot could not be verified')
        update.save(directory,plan,'snapshot-ready',retained_inventory=retained['inventory_sha256'])
        pending=update.state('recover','--revision',str(current['revision']),'--request',request,'--release-id',plan['previous'],'--snapshot',plan['snapshot'],'--reason',reason,'--at',str(int(time.time())))
        update.save(directory,plan,'restoring',pending_revision=pending['revision'])
        return update.recover_locked(directory,plan,identity)
    except Exception as error:
        failed_phase=plan['phase']
        try:
            result=update.recover_locked(directory,plan,identity)
            result.update(failed_phase=failed_phase,failure_type=type(error).__name__)
            return result
        except Exception as recovery_error:raise ValueError('Recorded rollback '+request+' requires explicit recovery') from recovery_error


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--operation',required=True);parser.add_argument('--identity-file',type=Path,required=True);parser.add_argument('--reason',required=True)
    args=parser.parse_args();result=rollback(args.operation,args.identity_file,args.reason)
    print(json.dumps(result,indent=2))
    if result['state']!='rolled-back':return 2


if __name__=='__main__':raise SystemExit(main())
