# SPDX-License-Identifier: AGPL-3.0-or-later
"""Prepare/verify a development runtime payload locally; signed installation is owned by M12."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
FORMAT = 'bbsbox.dev-runtime/v1'
TARGET = 'debian13-amd64'
MAX_MEMBER = 256 * 1024 * 1024
MAX_MANIFEST = 65536
STATIC = {
    'bbsbox.service': ROOT/'deploy/systemd/bbsbox.service',
    'node.example.json': ROOT/'deploy/node.example.json',
    'LICENSE.md': ROOT/'LICENSE.md',
    'LICENSES/AGPL-3.0-or-later.txt': ROOT/'LICENSES/AGPL-3.0-or-later.txt',
    'LICENSES/CC-BY-SA-4.0.txt': ROOT/'LICENSES/CC-BY-SA-4.0.txt',
}
MEMBERS = set(STATIC) | {'bbsbox-node'}


def encoded(value):
    return (json.dumps(value, sort_keys=True, indent=2, ensure_ascii=True)+'\n').encode()


def fingerprint(path):
    if path.is_symlink() or not path.is_file() or path.stat().st_size > MAX_MEMBER:
        raise ValueError('Payload member must be a bounded regular file')
    digest = hashlib.sha256()
    size = 0
    with path.open('rb') as source:
        while chunk := source.read(1024*1024):
            size += len(chunk)
            if size > MAX_MEMBER:
                raise ValueError('Payload member grew past its size limit')
            digest.update(chunk)
    return size, digest.hexdigest()


def check_elf(path):
    with path.open('rb') as source:
        header = source.read(64)
    if len(header) < 64 or header[:7] != b'\x7fELF\x02\x01\x01' or header[7] not in (0,3) or header[16:18] not in (b'\x02\0', b'\x03\0') or header[18:20] != b'\x3e\0':
        raise ValueError('Expected a little-endian ELF64 amd64 executable; Windows binaries cannot be installed as Debian payloads')


def copy_bounded(source, target):
    if not source.is_file() or source.stat().st_size > MAX_MEMBER:
        raise ValueError('Source member exceeds the payload limit')
    size = 0
    with source.open('rb') as incoming, target.open('xb') as outgoing:
        while chunk := incoming.read(1024*1024):
            size += len(chunk)
            if size > MAX_MEMBER:
                raise ValueError('Source member grew past the payload limit')
            outgoing.write(chunk)


def build(binary, destination, source_commit, source_dirty):
    if not re.fullmatch('[a-f0-9]{40}', source_commit) or type(source_dirty) is not bool:
        raise ValueError('Invalid source provenance')
    binary = Path(binary).resolve()
    destination = Path(destination).absolute()
    if not binary.is_file() or binary.stat().st_size>MAX_MEMBER:
        raise ValueError('Binary exceeds the payload limit')
    check_elf(binary)
    # Never replace an existing destination. The manifest is finalized last; partial output cannot verify.
    destination.mkdir(parents=True, exist_ok=False)
    records = []
    for name, source in sorted({**STATIC, 'bbsbox-node': binary}.items()):
        target = destination/name
        target.parent.mkdir(parents=True, exist_ok=True)
        copy_bounded(source, target)
        target.chmod(0o755 if name == 'bbsbox-node' else 0o644)
        size, digest = fingerprint(target)
        records.append(dict(path=name, bytes=size, sha256=digest))
    manifest = dict(format=FORMAT, target=TARGET, source_commit=source_commit, source_dirty=source_dirty, files=records)
    check_elf(destination/'bbsbox-node')
    (destination/'manifest.json').write_bytes(encoded(manifest))
    return verify(destination)


def verify(directory):
    directory = Path(directory).resolve()
    path = directory/'manifest.json'
    if path.is_symlink() or not path.is_file() or path.stat().st_size > MAX_MANIFEST:
        raise ValueError('Missing/oversized manifest')
    with path.open('rb') as source:
        raw = source.read(MAX_MANIFEST+1)
    if len(raw)>MAX_MANIFEST:
        raise ValueError('Manifest grew past its size limit')
    manifest = json.loads(raw)
    if not isinstance(manifest,dict) or encoded(manifest) != raw or set(manifest) != {'format', 'target', 'source_commit', 'source_dirty', 'files'}:
        raise ValueError('Malformed/noncanonical development manifest')
    if manifest['format'] != FORMAT or manifest['target'] != TARGET or not re.fullmatch('[a-f0-9]{40}', str(manifest['source_commit'])) or type(manifest['source_dirty']) is not bool:
        raise ValueError('Unsupported development payload/provenance')
    records = manifest['files']
    if not isinstance(records, list) or len(records) != len(MEMBERS):
        raise ValueError('Incorrect payload inventory')
    names = []
    for record in records:
        if not isinstance(record, dict) or set(record) != {'path', 'bytes', 'sha256'}:
            raise ValueError('Malformed member record')
        name = record['path']
        if not isinstance(name,str) or name not in MEMBERS or type(record['bytes']) is not int or not 0 <= record['bytes'] <= MAX_MEMBER or not re.fullmatch('[a-f0-9]{64}', str(record['sha256'])):
            raise ValueError('Unsupported member/path/size')
        names.append(name)
        member = directory/name
        if any(part.is_symlink() for part in [member, *member.parents] if part != directory.parent):
            raise ValueError('Symlinked payload member')
        size, digest = fingerprint(member)
        if (size, digest) != (record['bytes'], record['sha256']):
            raise ValueError('Payload bytes do not match the manifest')
    if names != sorted(MEMBERS):
        raise ValueError('Duplicate/unsorted payload inventory')
    observed = {p.relative_to(directory).as_posix() for p in directory.rglob('*') if p.is_file() or p.is_symlink()}
    if observed != MEMBERS | {'manifest.json'}:
        raise ValueError('Unlisted payload files')
    check_elf(directory/'bbsbox-node')
    return dict(format=FORMAT, target=TARGET, manifest_sha256=hashlib.sha256(raw).hexdigest(), files=len(records), source_commit=manifest['source_commit'], source_dirty=manifest['source_dirty'])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    prepare = sub.add_parser('build')
    prepare.add_argument('--binary', type=Path, required=True)
    prepare.add_argument('--output', type=Path, required=True)
    check = sub.add_parser('verify')
    check.add_argument('directory', type=Path)
    args = parser.parse_args()
    if args.command == 'build':
        revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT).decode().strip()
        dirty = bool(subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=normal'], cwd=ROOT))
        result = build(args.binary, args.output, revision, dirty)
    else:
        result = verify(args.directory)
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
