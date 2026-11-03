# SPDX-License-Identifier: AGPL-3.0-or-later
"""Prepare/inspect signed offline candidates; never install, select a service or alter node data."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading
import release_materials as materials

STATIC = {'bbsbox.service':'deploy/systemd/bbsbox-release.service', 'node.example.json':'deploy/node.example.json', 'LICENSE.md':'LICENSE.md', 'LICENSES/AGPL-3.0-or-later.txt':'LICENSES/AGPL-3.0-or-later.txt', 'LICENSES/CC-BY-SA-4.0.txt':'LICENSES/CC-BY-SA-4.0.txt'}
MAX_MEMBER = 2 * 1024 * 1024 * 1024
MAX_TOTAL = 4 * 1024 * 1024 * 1024
PACKAGE_COMMANDS = frozenset({'release-sign','release-inspect','release-stage','release-control-begin'})


def bounded_run(arguments, timeout=20):
    """Bound both pipes while running, including a child that writes without exiting."""
    process = subprocess.Popen([str(arg) for arg in arguments], stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    buffers = [bytearray(), bytearray()]
    failures = []
    def drain(index, stream, maximum):
        try:
            while raw := stream.read1(4096):
                if len(buffers[index]) + len(raw) > maximum:
                    failures.append('candidate command output exceeded its bound')
                    process.kill()
                    return
                buffers[index].extend(raw)
        except (OSError, ValueError):
            failures.append('candidate command output could not be read')
        finally:
            stream.close()
    readers = [threading.Thread(target=drain, args=(index, stream, maximum), daemon=True) for index, stream, maximum in [(0, process.stdout, 65536), (1, process.stderr, 4096)]]
    for reader in readers: reader.start()
    try:
        process.wait(timeout=timeout)
    except subprocess.TimeoutExpired as error:
        process.kill()
        process.wait(timeout=5)
        raise ValueError('candidate command exceeded its time limit') from error
    finally:
        for reader in readers: reader.join(timeout=1)
    if failures or any(reader.is_alive() for reader in readers):
        raise ValueError(failures[0] if failures else 'candidate command did not close its output pipes')
    if process.returncode:
        # Do not echo arbitrary command stderr (producer commands handle a private key).
        raise ValueError('candidate command failed with status '+str(process.returncode))
    return bytes(buffers[0])


def command(arguments):
    # Bundle commands hash/copy large prepared materials. A compiled-profile probe is much smaller.
    timeout = 1200 if len(arguments)>1 and str(arguments[1]) in PACKAGE_COMMANDS else 20
    raw = bounded_run(arguments, timeout=timeout)
    return materials.decoded(raw)


def public_info(path):
    info = materials.decoded(materials.read_file(path, 4096))
    if set(info) != {'format','public_key','key_id'} or info['format'] != 'bbsbox.release-public-key/v1':
        raise ValueError('Invalid producer public-key record')
    public = bytes.fromhex(info['public_key'])
    expected = 'release-key:'+hashlib.sha256(b'bbsbox.release-signing-key/v1\0'+public).hexdigest()
    if len(public) != 32 or public.hex() != info['public_key'] or info['key_id'] != expected:
        raise ValueError('Producer public-key identity does not match')
    return info


def copy_file(source, target):
    if materials.linked(source) or not source.is_file() or source.stat().st_size > MAX_MEMBER:
        raise ValueError('Candidate input must be a bounded regular file')
    target.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    size = 0
    digest = hashlib.sha256()
    with source.open('rb') as incoming, target.open('xb') as outgoing:
        while raw := incoming.read(1024*1024):
            size += len(raw)
            if size > MAX_MEMBER:
                raise ValueError('Candidate input grew past its bound')
            outgoing.write(raw)
            digest.update(raw)
        outgoing.flush()
        os.fsync(outgoing.fileno())
    target.chmod(0o600)
    return size, digest.hexdigest()


def committed_assets(material_directory):
    wanted = {'source/'+path: name for name,path in STATIC.items()}
    found = {}
    for path, raw, _ in materials.tar_entries(material_directory/'source.tar', materials.SOURCE_FILES+1, materials.SOURCE_MEMBER, materials.SOURCE_TOTAL):
        if path in wanted:
            found[wanted.pop(path)] = raw
    if wanted:
        raise ValueError('Committed source lacks required release assets')
    return found


def prepare(tool, binary, material_directory, commit, evidence_path, key_file, key_info, sequence, created_at, output, next_public=None):
    if not 1 <= sequence <= 2**64-1 or not -(2**63) <= created_at < 2**63:
        raise ValueError('Invalid release sequence/time')
    verified = materials.verify(material_directory, commit)
    evidence = materials.decoded(materials.read_file(evidence_path, 65536))
    if evidence.get('format') != 'bbsbox.offline-material-rebuild/v1' or evidence.get('source_commit') != commit or evidence.get('materials_sha256') != verified['sha256'] or evidence.get('binary_sha256') != materials.file_digest(binary) or not evidence.get('compiled_profile_matches_candidate'):
        raise ValueError('Candidate does not match its material rebuild evidence')
    profile = command([binary, '--release-profile'])
    if profile != evidence.get('compiled_profile') or profile.get('format') != 'bbsbox.release-runtime/v1' or profile.get('target') != 'debian13-amd64':
        raise ValueError('Candidate compiled profile differs from its rebuild evidence')
    authority = public_info(key_info)
    next_key=None
    if next_public is not None:
        successor=bytes.fromhex(next_public)
        if len(successor)!=32 or successor.hex()!=next_public or next_public==authority['public_key']:raise ValueError('Next release authority must be a different canonical public key')
        next_key=list(successor)
    output = Path(output).absolute()
    materials.directory(output.parent)
    output.mkdir(mode=0o700)
    _, binary_hash = copy_file(binary, output/'bbsbox-node')
    if binary_hash != evidence['binary_sha256']:
        raise ValueError('Candidate binary changed while copying')
    for name, raw in committed_assets(material_directory).items():
        target = output/name
        target.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        with target.open('xb') as destination:
            destination.write(raw)
            destination.flush()
            os.fsync(destination.fileno())
        target.chmod(0o600)
    for name in ('source.tar','dependencies.tar','notices.json'):
        _, copied_hash = copy_file(material_directory/name, output/'materials'/name)
        if copied_hash != verified['sha256'][name]:
            raise ValueError('Candidate materials changed while copying')
    records = []
    total = 0
    for path in sorted(p for p in output.rglob('*') if p.is_file()):
        size = path.stat().st_size
        total += size
        records.append({'path':path.relative_to(output).as_posix(),'size':size,'sha256':materials.file_digest(path)})
    if total > MAX_TOTAL:
        raise ValueError('Candidate exceeds total byte budget')
    manifest = dict(format='bbsbox.release-manifest/v1',issuer=authority['key_id'],sequence=sequence,version=profile['version'],target=profile['target'],created_at=created_at,source_commit=commit,core_schema=profile['core_schema'],readable_core_schemas=profile['readable_core_schemas'],module_migrations=profile['module_migrations'],files=records,next_key=next_key)
    with tempfile.TemporaryDirectory(prefix='release-input-',dir=output.parent) as temporary:
        path=Path(temporary)/'input.json'
        path.write_bytes(materials.encoded(manifest))
        report=command([tool,'release-sign','--bundle',output,'--manifest',path,'--key-file',key_file])
    if report.get('source_commit') != commit or report.get('expected_profile') != profile or report.get('issuer') != authority['key_id']:
        raise ValueError('Signed candidate inspection differs from prepared inputs')
    return report


def inspect(tool, bundle, public, output):
    # Staging authenticates all copied bytes before any candidate code is executable.
    report=command([tool,'release-stage','--bundle',bundle,'--public-key',public,'--output',output])
    if report.get('format')!='bbsbox.release-inspection/v1' or report.get('installation_authority') is not False:
        raise ValueError('Unsupported public release inspection')
    material_report=materials.verify(Path(output)/'materials',report['source_commit'])
    for name, raw in committed_assets(Path(output)/'materials').items():
        if materials.read_file(Path(output)/name, materials.SOURCE_MEMBER) != raw:
            raise ValueError('Release assets differ from committed source')
    binary=Path(output)/'bbsbox-node'
    binary.chmod(0o700)
    profile=command([binary,'--release-profile'])
    if profile != report['expected_profile']:
        raise ValueError('Authenticated release claims differ from actual compiled profile')
    return dict(format='bbsbox.verified-candidate/v1',release_id=report['release_id'],sequence=report['sequence'],source_commit=report['source_commit'],compiled_profile_matched=True,materials=material_report,installation_authority=False)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    commands=parser.add_subparsers(dest='command',required=True)
    prepare_parser=commands.add_parser('prepare')
    prepare_parser.add_argument('--tool',type=Path,required=True)
    prepare_parser.add_argument('--binary',type=Path,required=True)
    prepare_parser.add_argument('--materials',type=Path,required=True)
    prepare_parser.add_argument('--source-commit',required=True)
    prepare_parser.add_argument('--rebuild-evidence',type=Path,required=True)
    prepare_parser.add_argument('--key-file',type=Path,required=True)
    prepare_parser.add_argument('--key-info',type=Path,required=True)
    prepare_parser.add_argument('--sequence',type=int,required=True)
    prepare_parser.add_argument('--created-at',type=int,required=True)
    prepare_parser.add_argument('--next-public-key')
    prepare_parser.add_argument('--output',type=Path,required=True)
    inspect_parser=commands.add_parser('inspect')
    inspect_parser.add_argument('--tool',type=Path,required=True)
    inspect_parser.add_argument('--bundle',type=Path,required=True)
    inspect_parser.add_argument('--public-key',required=True)
    inspect_parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    if args.command=='prepare':
        result=prepare(args.tool.resolve(),args.binary.resolve(),args.materials,args.source_commit,args.rebuild_evidence,args.key_file,args.key_info,args.sequence,args.created_at,args.output,args.next_public_key)
    else:
        result=inspect(args.tool.resolve(),args.bundle,args.public_key,args.output)
    print(json.dumps(result,indent=2))


if __name__=='__main__': main()
