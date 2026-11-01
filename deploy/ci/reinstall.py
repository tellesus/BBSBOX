# SPDX-License-Identifier: AGPL-3.0-or-later
"""Repeat offline first installation on fresh prepared OS layers while retaining the source node."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

ROOT=Path(__file__).resolve().parents[2]


def fingerprint(path):
    digest=hashlib.sha256()
    with path.open('rb') as stream:
        while chunk:=stream.read(1024*1024):digest.update(chunk)
    return digest.hexdigest()


def exercise(root,output):
    root=root.resolve();work=(ROOT/'work').resolve()
    if root==work or not root.is_relative_to(work):raise ValueError('Use only the prepared repository work/ OS tree')
    output=output.absolute()
    if output!=output.resolve() or not output.is_relative_to(work) or output.exists():raise ValueError('Use a fresh evidence file beneath repository work/')
    installed=json.loads((root/'etc/bbsbox/installation.json').read_text())
    if installed['format']!='bbsbox.development-install/v1':raise ValueError('Expected the developer fixture')
    sources=[root/name for name in ['var/lib/bbsbox/node.sqlite','etc/bbsbox/node.json','etc/bbsbox/tls/ca.pem']]
    if any(not path.resolve().is_relative_to(root) or not path.is_file() for path in sources):raise ValueError('Source state escaped the prepared OS tree')
    before=[fingerprint(path) for path in sources]
    identities=[installed['node_id']]
    for _ in range(2):
        result=subprocess.run(['docker','run','--rm','--pull=never','--network=none','--env','SYSTEMD_OFFLINE=1','--entrypoint','python3','bbsbox-prepared-fixture','deploy/install_development.py','--bundle','payload','--development','--enable','--name','Fresh media fixture'],check=True,capture_output=True,timeout=45)
        information=json.loads(result.stdout)
        if information.get('format')!='bbsbox.development-install/v1' or not re.fullmatch(r'node:[a-f0-9]{64}',information.get('node_id','')):raise ValueError('Unexpected fresh-install result')
        identities.append(information['node_id'])
    assert len(set(identities))==3,'Fresh media reused a source node identity'
    assert before==[fingerprint(path) for path in sources],'Retained source state changed'
    report=dict(status='passed',fresh_offline_installs=2,three_distinct_node_ids=identities,retained_source_state_unchanged=True,scope='Fresh prepared developer OS layers with networking disabled; coordinated original-node restore is M12')
    with output.open('x',encoding='utf-8') as stream:stream.write(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2))


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--root',type=Path,required=True);parser.add_argument('--output',type=Path,default=ROOT/'work/reinstall-evidence.json');args=parser.parse_args();exercise(args.root,args.output)


if __name__=='__main__':main()
