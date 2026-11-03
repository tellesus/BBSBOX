# SPDX-License-Identifier: AGPL-3.0-or-later
"""Exercise fresh recovery inside disposable Docker with only read-only operator inputs."""
import json
from pathlib import Path
import subprocess
import sys

MEDIA=Path('/recovery-inputs')
sys.path.insert(0,str(MEDIA/'deploy'))
import install_release
from restore_provisioning import Request


def exercise():
    request=json.loads((MEDIA/'request.json').read_text())
    layout=install_release.base.Layout(Path('/'))
    def provision(node,minimum):
        source=Request(MEDIA/'source.age',MEDIA/'operator.key',node,'lost','CI removed the original appliance from service',minimum)
        return install_release.install(MEDIA/'bundle',MEDIA/'bootstrap',request['public_key'],None,True,recovery=source)
    install_release.check_fresh(layout)
    try:provision(request['node'],7)
    except ValueError as error:assert 'minimum sequence' in str(error)
    else:raise AssertionError('Accepted a release below the independently supplied minimum')
    install_release.check_fresh(layout)
    try:provision('node:'+'11'*32,request['minimum_sequence'])
    except ValueError:pass
    else:raise AssertionError('Accepted the archive as another node')
    install_release.check_fresh(layout)
    recovered=json.loads(subprocess.check_output([sys.executable,str(MEDIA/'deploy/recover_release.py'),'--bundle',str(MEDIA/'bundle'),'--verifier',str(MEDIA/'bootstrap'),'--public-key',request['public_key'],'--minimum-sequence',str(request['minimum_sequence']),'--archive',str(MEDIA/'source.age'),'--identity-file',str(MEDIA/'operator.key'),'--node',request['node'],'--source','lost','--reason','CI removed the original appliance from service','--enable'],timeout=240))
    assert recovered['source_inventory_sha256']==request['inventory_sha256'] and recovered['migration_applied'] is True
    assert recovered['installation']['release_sequence']==6 and recovered['network']=='loopback'
    recovered.update(minimum_sequence_enforced=True,wrong_node_refused=True,failed_preflight_left_target_fresh=True)
    print(json.dumps(recovered))


if __name__=='__main__':exercise()
