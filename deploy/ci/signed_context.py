# SPDX-License-Identifier: AGPL-3.0-or-later
"""Copy only public signed payload/bootstrap inputs into a fresh disposable Docker context."""
import argparse
from pathlib import Path
import sys

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'tools'))
import release_candidate as candidate


def context(fixture,verifier,output):
    fixture=fixture.resolve(strict=True);verifier=verifier.resolve(strict=True)
    output=output.absolute()
    work=(ROOT/'work').resolve()
    if output!=output.resolve() or not output.is_relative_to(work) or output==work:raise ValueError('Signed context must stay below repository work/')
    public=candidate.public_info(fixture/'public.json')
    candidate.command([verifier,'release-inspect','--bundle',fixture/'bundle','--public-key',public['public_key']])
    output.mkdir(mode=0o755);output.chmod(0o755)
    sources={'Dockerfile':ROOT/'deploy/ci/Dockerfile.release','bootstrap/bbsbox-operator':verifier}
    for name in ['install_release.py','update_release.py','rollback_release.py','install_development.py','backup_development.py','backup_node.py','enrollment.py','configure_network.py','network_profile.py','local_tls.py','renew_tls.py','maintenance_lock.py','diagnose.py']:
        sources['deploy/'+name]=ROOT/'deploy'/name
    for name in ['runtime_bundle.py','release_candidate.py','release_materials.py']:
        sources['tools/'+name]=ROOT/'tools'/name
    for path in (fixture/'bundle').rglob('*'):
        if path.is_file():sources['bundle/'+path.relative_to(fixture/'bundle').as_posix()]=path
    sources['next-manifest.cose']=fixture/'next/manifest.cose'
    sources['rotation-public.json']=fixture/'rotation-public.json'
    for name in ['upgrade_rejecting','upgrade_interrupted','upgrade_successor']:
        # Other committed assets/materials equal baseline; reconstruct public links in-guest.
        for member in ['manifest.cose','bbsbox-node']:sources[name+'/'+member]=fixture/name/member
    for name,source in sources.items():
        target=output/name;target.parent.mkdir(parents=True,exist_ok=True,mode=0o755)
        candidate.copy_file(source,target);target.chmod(0o755 if name=='bootstrap/bbsbox-operator' else 0o644)
        for parent in target.parents:
            if parent==output:break
            parent.chmod(0o755)
    (output/'public-key.txt').write_text(public['public_key']+'\n',encoding='ascii');(output/'public-key.txt').chmod(0o644)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ['fixture','verifier','output']:parser.add_argument('--'+name,type=Path,required=True)
    args=parser.parse_args();context(args.fixture,args.verifier,args.output)


if __name__=='__main__':main()
