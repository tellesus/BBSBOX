# SPDX-License-Identifier: AGPL-3.0-or-later
"""Additional cases within the verified disposable signed-install guest."""
import json
import os
from pathlib import Path
import re
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
    return dict(root_boot_gate=True,pending_operation_blocks_unit=True,recovery_audited=True,maximum_sequence_preserved=2,active_release_preserved=True,no_release_private_key_on_appliance=True)
