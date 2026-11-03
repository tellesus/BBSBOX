# SPDX-License-Identifier: AGPL-3.0-or-later
"""Exercise producer/public inspection with ephemeral keys in a private Linux network namespace."""
import argparse
import json
import os
from pathlib import Path
import socket
import sys

ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'tools'))
import release_candidate as candidate
import release_materials as materials


def check(tool,binary,material_directory,commit,rebuild_evidence,root,output):
    if sys.platform!='linux' or os.geteuid()!=0 or {name for _,name in socket.if_nameindex()}!={'lo'} or os.readlink('/proc/self/ns/net')==os.readlink('/proc/1/ns/net'):
        raise ValueError('Candidate fixture requires its own root-owned loopback-only namespace')
    root=root.absolute()
    if root==ROOT/'work' or not root.parent.resolve().is_relative_to((ROOT/'work').resolve()):
        raise ValueError('Candidate fixture output must stay below the repository work directory')
    root.mkdir(mode=0o700)
    tool=tool.resolve(strict=True);binary=binary.resolve(strict=True)
    key=root/'ephemeral-release.key';key_info=root/'public.json'
    public=candidate.command([tool,'release-keygen','--output',key])
    key_info.write_bytes(materials.encoded(public))
    bundle=root/'bundle'
    prepared=candidate.prepare(tool,binary,material_directory,commit,rebuild_evidence,key,key_info,1,0,bundle)
    verified=candidate.inspect(tool,bundle,public['public_key'],root/'verified')
    if prepared['release_id']!=verified['release_id'] or verified['installation_authority'] is not False:
        raise ValueError('Candidate round trip did not preserve its exact identity/boundary')
    wrong=candidate.command([tool,'release-keygen','--output',root/'other-ephemeral.key'])
    (root/'rotation-public.json').write_bytes(materials.encoded(wrong))
    try: candidate.inspect(tool,bundle,wrong['public_key'],root/'wrong-key')
    except ValueError: pass
    else: raise AssertionError('wrong release authority accepted')
    if (root/'wrong-key').exists(): raise AssertionError('wrong authority created staging output')
    notices=bundle/'materials/notices.json';original=notices.read_bytes();notices.write_bytes(original+b'\n')
    try: candidate.inspect(tool,bundle,public['public_key'],root/'tampered')
    except ValueError: pass
    else: raise AssertionError('tampered material accepted')
    if (root/'tampered').exists(): raise AssertionError('tampered content created staging output')
    notices.write_bytes(original)
    # Keep large public fixture files immutable through hard links; staged outputs are fresh copies.
    claims=root/'false-claims';claims.mkdir(mode=0o700)
    files=[]
    for path in sorted(p for p in bundle.rglob('*') if p.is_file() and p.name!='manifest.cose'):
        name=path.relative_to(bundle);target=claims/name;target.parent.mkdir(parents=True,exist_ok=True,mode=0o700);os.link(path,target)
        files.append(dict(path=name.as_posix(),size=path.stat().st_size,sha256=materials.file_digest(path)))
    profile=json.loads(json.dumps(prepared['expected_profile']))
    manifest=dict(format='bbsbox.release-manifest/v1',issuer=public['key_id'],sequence=2,version=profile['version'],target=profile['target'],created_at=0,source_commit=commit,core_schema=profile['core_schema'],readable_core_schemas=profile['readable_core_schemas'],module_migrations=profile['module_migrations'],files=files,next_key=None)
    # A valid next-sequence manifest travels into the service fixture without its signing key.
    next_bundle=root/'next';next_bundle.mkdir(mode=0o700)
    for path in claims.rglob('*'):
        if path.is_file():
            target=next_bundle/path.relative_to(claims);target.parent.mkdir(parents=True,exist_ok=True,mode=0o700);os.link(path,target)
    next_input=root/'next.json';next_input.write_bytes(materials.encoded(manifest))
    candidate.command([tool,'release-sign','--bundle',next_bundle,'--manifest',next_input,'--key-file',key])
    for sequence,name in [(3,'upgrade_rejecting'),(4,'upgrade_interrupted'),(5,'upgrade_successor')]:
        executable=binary.parent/'examples'/name
        evidence=rebuild_evidence.parent/('offline-'+name+'.json')
        candidate.prepare(tool,executable,material_directory,commit,evidence,key,key_info,sequence,0,root/name,wrong['public_key'] if name=='upgrade_successor' else None)
    candidate.prepare(tool,binary.parent/'examples/upgrade_successor',material_directory,commit,rebuild_evidence.parent/'offline-upgrade_successor.json',root/'other-ephemeral.key',root/'rotation-public.json',6,0,root/'source_loss_successor')
    manifest['module_migrations'][0]['sha256']='00'*32
    input_path=root/'false-claims.json';input_path.write_bytes(materials.encoded(manifest))
    candidate.command([tool,'release-sign','--bundle',claims,'--manifest',input_path,'--key-file',key])
    try: candidate.inspect(tool,claims,public['public_key'],root/'false-profile')
    except ValueError: pass
    else: raise AssertionError('authenticated false compiled-profile claim accepted')
    report=dict(format='bbsbox.signed-candidate-fixture/v1',source_commit=commit,release_id=verified['release_id'],dependencies=verified['materials']['dependencies'],ephemeral_test_authority=True,private_copy=True,compiled_profile_matched=True,wrong_authority_rejected=True,tampered_material_rejected=True,authenticated_false_profile_rejected=True,network='isolated-loopback-only',installation_authority=False)
    with output.open('x',encoding='utf-8') as destination:json.dump(report,destination,indent=2);destination.write('\n')
    output.chmod(0o644) # Only this public evidence is uploaded; private test keys remain excluded.
    return report


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ('tool','binary','materials','rebuild-evidence','root','output'):parser.add_argument('--'+name,type=Path,required=True)
    parser.add_argument('--source-commit',required=True)
    args=parser.parse_args()
    print(json.dumps(check(args.tool,args.binary,args.materials,args.source_commit,args.rebuild_evidence,args.root,args.output),indent=2))


if __name__=='__main__':main()
