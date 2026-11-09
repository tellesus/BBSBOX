# SPDX-License-Identifier: AGPL-3.0-or-later
"""Prepare and verify offline source/dependency materials; never install or sign a release."""
import argparse
import base64
from concurrent.futures import ThreadPoolExecutor
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess
import tarfile
import tomllib
import urllib.request

FORMAT = 'bbsbox.release-materials/v1'
SOURCE_FILES = 2048
SOURCE_MEMBER = 8 * 1024 * 1024
SOURCE_TOTAL = 64 * 1024 * 1024
DEPENDENCY_FILES = 100000
DEPENDENCY_MEMBER = 128 * 1024 * 1024
DEPENDENCY_TOTAL = 2 * 1024 * 1024 * 1024
MAX_METADATA = 8 * 1024 * 1024
REGISTRY = 'registry+https://github.com/rust-lang/crates.io-index'
WEB_LOCK = 'web/shell/package-lock.json'
WEB_NOTICE = 'deploy/materials/web-mit-notices.txt'
WEB_BUILD = ("For the browser shell, prepare the pinned Node.js/npm tools on Debian 13 amd64.\n"
             "From source/web/shell, run npm cache add --offline --cache ../../../npm-cache ../../../npm-packages/*.tgz,\n"
             "then npm ci --offline --cache ../../../npm-cache --ignore-scripts --no-audit --no-fund and npm run verify.\n").encode()
WEB_PACKAGE = 24 * 1024 * 1024
BUILD = ('Use the pinned rust-toolchain.toml and prepared native compiler/linker on Debian 13 amd64.\n'
         'Extract source.tar and dependencies.tar into one fresh directory. From source/, run:\n'
         'cargo --config \'source.crates-io.replace-with="vendored-sources"\' '
         '--config \'source.vendored-sources.directory="../vendor"\' '
         'build --frozen -p bbsbox-node-runtime\n'
         'The toolchain and base OS are prepared separately; the appliance does not download them.\n').encode()


def encoded(value):
    return (json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=True) + '\n').encode()


def unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError('Duplicate JSON field')
        result[key] = value
    return result


def decoded(raw):
    if len(raw) > MAX_METADATA:
        raise ValueError('Oversized material metadata')
    return json.loads(raw, object_pairs_hook=unique)


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def file_digest(path):
    result = hashlib.sha256()
    with path.open('rb') as source:
        while raw := source.read(1024 * 1024):
            result.update(raw)
    return result.hexdigest()


def git_hash(kind, raw):
    # Match this repository's Git object IDs; outer release commitments use SHA-256.
    return hashlib.sha1(kind.encode() + b' ' + str(len(raw)).encode() + b'\0' + raw).hexdigest()


def safe_name(name):
    if not isinstance(name, str) or not name or len(name.encode()) > 255 or '\\' in name:
        raise ValueError('Unsafe material path')
    parts = name.split('/')
    if len(parts) > 32 or any(not p or p in ('.', '..') or ':' in p or p.endswith(('.', ' ')) or any(ord(c) < 32 for c in p) for p in parts):
        raise ValueError('Unsafe material path')
    return name


def linked(path):
    metadata = path.lstat()
    return stat.S_ISLNK(metadata.st_mode) or bool(getattr(metadata, 'st_file_attributes', 0) & getattr(stat, 'FILE_ATTRIBUTE_REPARSE_POINT', 0))


def directory(path):
    path = Path(os.path.abspath(path))
    if any(linked(p) or not p.is_dir() for p in [path, *path.parents]):
        raise ValueError('Material directories must have no linked ancestors')
    return path


def read_file(path, maximum):
    if linked(path) or not path.is_file() or path.stat().st_size > maximum:
        raise ValueError('Material member must be a bounded regular file')
    with path.open('rb') as source:
        chunks = []
        size = 0
        while raw := source.read(min(1024 * 1024, maximum + 1 - size)):
            size += len(raw)
            if size > maximum:
                raise ValueError('Material member grew past its limit')
            chunks.append(raw)
    return b''.join(chunks)


def add(archive, name, raw, mode=0o644):
    info = tarfile.TarInfo(safe_name(name))
    info.size = len(raw)
    info.mode = mode
    info.mtime = 0
    archive.addfile(info, io.BytesIO(raw))


def tar_entries(path, maximum_files, maximum_member, maximum_total):
    """Raw bounded USTAR only: no generic unpacking or extension-header allocation."""
    if linked(path) or not path.is_file() or path.stat().st_size > maximum_total + maximum_files * 1024 + 10240:
        raise ValueError('Oversized/nonregular material archive')
    seen = set()
    total = 0
    with path.open('rb') as source:
        while True:
            header = source.read(512)
            if header == bytes(512):
                if source.read(512) != bytes(512):
                    raise ValueError('Missing material archive terminator')
                padding = source.read(10241)
                if len(padding) > 10240 or any(padding):
                    raise ValueError('Extra material archive data')
                return
            if len(header) != 512 or header[257:263] != b'ustar\0':
                raise ValueError('Unsupported/truncated material archive')
            info = tarfile.TarInfo.frombuf(header, 'utf-8', 'strict')
            name = safe_name(info.name)
            if not info.isreg() or info.type != tarfile.REGTYPE or info.linkname or name in seen or len(seen) >= maximum_files or not 0 <= info.size <= maximum_member:
                raise ValueError('Unsupported material member/type/size')
            total += info.size
            if total > maximum_total:
                raise ValueError('Material archive exceeds byte budget')
            seen.add(name)
            raw = source.read(info.size)
            padding_size = (-info.size) % 512
            padding = source.read(padding_size)
            if len(raw) != info.size or len(padding) != padding_size or any(padding):
                raise ValueError('Truncated/noncanonical material member')
            yield name, raw, info.mode


def run_git(repository, *arguments, limit=MAX_METADATA):
    result = subprocess.run(['git', '-C', str(repository), *arguments], check=True, capture_output=True)
    if len(result.stdout) > limit:
        raise ValueError('Oversized Git object')
    return result.stdout


def source_proof(repository, commit):
    commit = run_git(repository, 'rev-parse', '--verify', commit + '^{commit}').decode().strip()
    if not re.fullmatch('[a-f0-9]{40}', commit):
        raise ValueError('Expected a SHA-1 Git commit ID')
    raw_commit = run_git(repository, 'cat-file', 'commit', commit)
    tree = raw_commit.split(b'\n', 1)[0].removeprefix(b'tree ').decode()
    trees = {}
    pending = [tree]
    while pending:
        sha = pending.pop()
        if sha in trees:
            continue
        if len(trees) >= SOURCE_FILES:
            raise ValueError('Too many source trees')
        raw = run_git(repository, 'cat-file', 'tree', sha)
        trees[sha] = raw.hex()
        for mode, _, child in tree_entries(raw):
            if mode == '40000':
                pending.append(child)
    proof = {'commit': raw_commit.hex(), 'trees': trees}
    return commit, proof, proof_files(proof, commit)


def tree_entries(raw):
    names = set()
    offset = 0
    while offset < len(raw):
        space = raw.index(b' ', offset)
        zero = raw.index(b'\0', space)
        mode = raw[offset:space].decode('ascii')
        name = raw[space + 1:zero].decode('utf-8')
        safe_name(name)
        if '/' in name or name in names or mode not in ('40000', '100644', '100755') or zero + 21 > len(raw):
            raise ValueError('Unsupported source tree entry')
        names.add(name)
        yield mode, name, raw[zero + 1:zero + 21].hex()
        offset = zero + 21


def proof_files(proof, expected_commit):
    if set(proof) != {'commit', 'trees'} or not isinstance(proof['trees'], dict) or len(proof['trees']) > SOURCE_FILES:
        raise ValueError('Invalid source proof')
    raw_commit = bytes.fromhex(proof['commit'])
    if git_hash('commit', raw_commit) != expected_commit or not re.match(rb'tree [a-f0-9]{40}\n', raw_commit):
        raise ValueError('Source commit proof does not match release')
    trees = {sha: bytes.fromhex(raw) for sha, raw in proof['trees'].items()}
    if any(git_hash('tree', raw) != sha for sha, raw in trees.items()):
        raise ValueError('Source tree object hash mismatch')
    pending = [('', raw_commit[5:45].decode(), frozenset())]
    visited = set()
    expanded = 0
    files = {}
    while pending:
        expanded += 1
        if expanded > SOURCE_FILES * 2:
            raise ValueError('Too many expanded source trees')
        prefix, sha, ancestors = pending.pop()
        if sha in ancestors or sha not in trees:
            raise ValueError('Missing/cyclic source tree')
        visited.add(sha)
        for mode, name, child in tree_entries(trees[sha]):
            path = safe_name(prefix + name)
            if mode == '40000':
                pending.append((path + '/', child, ancestors | {sha}))
            else:
                if len(files) >= SOURCE_FILES or path in files:
                    raise ValueError('Too many/duplicate source files')
                files[path] = (child, 0o755 if mode == '100755' else 0o644)
    if visited != set(trees):
        raise ValueError('Unreferenced source proof objects')
    return files


def build_source(repository, commit, destination):
    commit, proof, files = source_proof(repository, commit)
    total = 0
    with tarfile.open(destination, 'x', format=tarfile.USTAR_FORMAT, encoding='utf-8') as archive:
        add(archive, 'source-proof.json', encoded(proof))
        for name, (sha, mode) in sorted(files.items()):
            raw = run_git(repository, 'cat-file', 'blob', sha, limit=SOURCE_MEMBER)
            total += len(raw)
            if total > SOURCE_TOTAL:
                raise ValueError('Source exceeds byte budget')
            add(archive, 'source/' + name, raw, mode)
    return commit


def inspect_source(path, commit, consume=None, *, extra_paths=()):
    entries = iter(tar_entries(path, SOURCE_FILES + 1, SOURCE_MEMBER, SOURCE_TOTAL))
    name, raw, _ = next(entries)
    if name != 'source-proof.json':
        raise ValueError('Source proof must be first')
    proof = decoded(raw)
    if encoded(proof) != raw:
        raise ValueError('Noncanonical source proof')
    remaining = proof_files(proof, commit)
    selected = {}
    for name, raw, mode in entries:
        if not name.startswith('source/'):
            raise ValueError('Unlisted source member')
        relative = name[7:]
        expected = remaining.pop(relative, None)
        if expected != (git_hash('blob', raw), mode):
            raise ValueError('Source content/mode does not match commit tree')
        if consume:
            consume(name, raw, mode)
        if relative in ('Cargo.lock', WEB_LOCK, WEB_NOTICE, 'deploy/materials/supplements.json') or relative in extra_paths or relative.startswith('deploy/materials/licenses/'):
            selected[relative] = raw
    if remaining or 'Cargo.lock' not in selected:
        raise ValueError('Incomplete source archive')
    return selected


def locked_packages(raw):
    packages = tomllib.loads(raw.decode())['package']
    registry = {}
    for package in packages:
        if not package.get('source'):
            continue
        if package['source'] != REGISTRY or not re.fullmatch('[a-f0-9]{64}', package.get('checksum', '')):
            raise ValueError('Unsupported dependency source/checksum')
        name = safe_name(package['name'] + '-' + package['version'])
        if '/' in name or name in registry or len(registry) >= 1024:
            raise ValueError('Invalid dependency package identity')
        registry[name] = package
    return registry


def license_path(name):
    path = PurePosixPath(name)
    return path.name.upper().startswith(('LICENSE', 'COPYING', 'NOTICE', 'COPYRIGHT', 'AUTHORS')) or any(p.upper() == 'LICENSES' for p in path.parts)


def locked_web_packages(raw):
    if raw is None:
        return {}
    lock = decoded(raw)
    entries = lock.get('packages', {})
    if lock.get('lockfileVersion') != 3 or not 2 <= len(entries) <= 512:
        raise ValueError('Unsupported browser package lock')
    packages = {}
    for path, entry in entries.items():
        if not path or 'linux' not in entry.get('os', ['linux']) or 'x64' not in entry.get('cpu', ['x64']):
            continue
        name = path.rsplit('node_modules/', 1)[-1]
        version = entry.get('version', '')
        if not re.fullmatch(r'(?:@[a-z0-9_.-]+/)?[a-z0-9_.-]+', name) or not re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+(?:-[a-zA-Z0-9.-]+)?', version):
            raise ValueError('Unsupported browser dependency identity')
        registry = f'https://registry.npmjs.org/{name}/-/{name.rsplit("/", 1)[-1]}-{version}.tgz'
        integrity = entry.get('integrity', '')
        if entry.get('resolved') != registry or not re.fullmatch(r'sha512-[A-Za-z0-9+/]{86}==', integrity):
            raise ValueError('Unsupported browser dependency source or integrity')
        checksum = base64.b64decode(integrity[7:], validate=True).hex()
        package = (name, version, registry, integrity)
        if checksum in packages and packages[checksum] != package:
            raise ValueError('Ambiguous browser dependency identity')
        packages[checksum] = package
    if not packages:
        raise ValueError('Browser package lock has no Debian dependencies')
    return packages


def web_package(raw, checksum, package):
    name, version, registry, integrity = package
    if not raw or len(raw) > WEB_PACKAGE or hashlib.sha512(raw).hexdigest() != checksum:
        raise ValueError('Browser package checksum mismatch: ' + name)
    metadata = None
    licenses = []
    total = 0
    paths = set()
    with tarfile.open(fileobj=io.BytesIO(raw), mode='r|gz') as archive:
        for item in archive:
            path = safe_name(item.name)
            total += item.size
            if path in paths or len(paths) >= 20000 or not (item.isreg() or item.isdir()) or total > 128 * 1024 * 1024 or item.size > WEB_PACKAGE:
                raise ValueError('Unsupported browser package member: ' + name)
            paths.add(path)
            if item.isreg() and path.count('/') == 1 and path.endswith('/package.json'):
                if metadata is not None or item.size > 256 * 1024:
                    raise ValueError('Invalid browser package metadata')
                metadata = decoded(archive.extractfile(item).read())
            if item.isreg() and license_path(path):
                if not 0 < item.size <= 512 * 1024:
                    raise ValueError('Invalid browser license member')
                licenses.append({'path': path, 'sha256': digest(archive.extractfile(item).read())})
    if not metadata or metadata.get('name') != name or metadata.get('version') != version or not isinstance(metadata.get('license'), str) or not metadata['license']:
        raise ValueError('Browser package metadata differs from its lock: ' + name)
    return dict(name=name, version=version, source=registry, integrity=integrity, license=metadata['license'], license_files=sorted(licenses, key=lambda record: record['path']))


def web_path(checksum):
    name = base64.urlsafe_b64encode(bytes.fromhex(checksum)).decode().rstrip('=')
    return f'npm-packages/{name}.tgz'


def web_notices(records, selected):
    if not records:
        return {}
    by_identity = {(record['name'], record['version']): record for record in records}
    supplemented = {
        ('@napi-rs/lzma-linux-x64-gnu', '1.5.1'),
        ('@polka/url', '1.0.0-next.29'),
        ('sirv', '3.0.2'),
        ('is-reference', '3.0.3'),
        ('locate-character', '3.0.0'),
    }
    for identity, record in by_identity.items():
        if record['license_files']:
            continue
        name, version = identity
        if name.startswith(('@esbuild/', '@rollup/')):
            parent = 'esbuild' if name.startswith('@esbuild/') else 'rollup'
            inherited = by_identity.get((parent, version), {})
            if inherited.get('license') == record['license'] and inherited.get('license_files'):
                record['license_package'] = parent + '@' + version
                continue
        if identity not in supplemented or record['license'] != 'MIT' or not selected.get(WEB_NOTICE):
            raise ValueError('Browser dependency needs a reviewed license supplement: ' + name)
        record['license_supplement'] = WEB_NOTICE
    return {'web_dependencies': sorted(records, key=lambda record: (record['name'], record['version']))}


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def download_web_package(package):
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
    with opener.open(package[2], timeout=30) as response:
        raw = response.read(WEB_PACKAGE + 1)
    if len(raw) > WEB_PACKAGE:
        raise ValueError('Browser dependency exceeds package limit')
    return raw


def inspect_dependencies(entries, selected, consume=None, *, extra_locks=(), instructions=None):
    packages = locked_packages(selected['Cargo.lock'])
    for raw in extra_locks:
        for name, package in locked_packages(raw).items():
            if name in packages and packages[name]['checksum'] != package['checksum']:
                raise ValueError('Conflicting locked dependency: ' + name)
            packages[name] = package
    web = {web_path(checksum): (checksum, package) for checksum, package in locked_web_packages(selected.get(WEB_LOCK)).items()}
    web_records = []
    build_instructions = instructions if instructions is not None else BUILD + (WEB_BUILD if web else b'')
    observed = {name: {} for name in packages}
    metadata = {name: {} for name in packages}
    licenses = {name: [] for name in packages}
    supplemental = {}
    build_seen = False
    for path, raw, mode in entries:
        if mode != 0o644:
            raise ValueError('Dependency material modes must be regular 0644')
        if path == 'offline-build.txt':
            if raw != build_instructions or build_seen:
                raise ValueError('Unsupported build instructions')
            build_seen = True
            if consume:
                consume(path, raw, mode)
            continue
        if path.startswith('npm-packages/'):
            package = web.pop(path, None)
            if package is None:
                raise ValueError('Unlisted browser dependency')
            web_records.append(web_package(raw, *package))
            if consume:
                consume(path, raw, mode)
            continue
        if path.startswith('supplemental-licenses/'):
            supplemental[path[22:]] = raw
            if consume:
                consume(path, raw, mode)
            continue
        parts = path.split('/', 2)
        if len(parts) != 3 or parts[0] != 'vendor' or parts[1] not in packages:
            raise ValueError('Unlisted dependency package/member')
        _, package, name = parts
        observed[package][name] = digest(raw)
        if consume:
            consume(path, raw, mode)
        if name in ('Cargo.toml', '.cargo-checksum.json'):
            if len(raw) > MAX_METADATA:
                raise ValueError('Oversized package metadata')
            metadata[package][name] = raw
        if license_path(name):
            licenses[package].append({'path': name, 'size': len(raw), 'sha256': digest(raw)})
    if not build_seen or web:
        raise ValueError('Missing offline build instructions or browser dependencies')
    supplement = decoded(selected.get('deploy/materials/supplements.json', b'{}'))
    if supplement.get('format') != 'bbsbox.license-supplements/v1' or set(supplement) != {'format', 'files', 'packages'}:
        raise ValueError('Missing/invalid reviewed license supplements')
    if set(supplemental) != set(supplement['files']):
        raise ValueError('Missing/extra supplemental license')
    for name, record in supplement['files'].items():
        if '/' in name or digest(supplemental[name]) != record['sha256'] or supplemental[name] != selected.get('deploy/materials/licenses/' + name):
            raise ValueError('Supplemental license differs from recorded source')
    notices = []
    for name, lock in sorted(packages.items()):
        meta = metadata[name]
        if set(meta) != {'Cargo.toml', '.cargo-checksum.json'}:
            raise ValueError('Missing vendored package metadata')
        checksum = decoded(meta['.cargo-checksum.json'])
        content = {key: value for key, value in observed[name].items() if key != '.cargo-checksum.json'}
        if not {'package', 'files'} <= set(checksum) or set(checksum) - {'package', 'files', '$comment'} or ('$comment' in checksum and (not isinstance(checksum['$comment'], str) or len(checksum['$comment']) > 1024)) or checksum['package'] != lock['checksum'] or checksum['files'] != content:
            raise ValueError('Vendored dependency content/checksum mismatch: ' + name)
        package = tomllib.loads(meta['Cargo.toml'].decode())['package']
        if package.get('name') != lock['name'] or package.get('version') != lock['version'] or not isinstance(package.get('license'), str) or not package['license']:
            raise ValueError('Dependency identity/license metadata mismatch')
        extra = supplement['packages'].get(name)
        if not licenses[name] and not extra:
            raise ValueError('Dependency needs a reviewed license supplement: ' + name)
        if extra and (extra['choice'] not in re.split(r'[^A-Za-z0-9.-]+', package['license']) or not extra['files'] or any(f not in supplemental for f in extra['files'])):
            raise ValueError('Supplemental license choice does not match package grant')
        notices.append({'name': lock['name'], 'version': lock['version'], 'source': REGISTRY, 'checksum': lock['checksum'], 'license': package['license'], 'authors': package.get('authors', []), 'repository': package.get('repository'), 'license_files': sorted(licenses[name], key=lambda f: f['path']), 'supplement': extra})
    if set(supplement['packages']) - set(packages):
        raise ValueError('Stale license supplements require review')
    return dict(dependencies=notices, **web_notices(web_records, selected))


def vendor_entries(vendor):
    vendor = directory(vendor)
    count = total = 0
    pending = [vendor]
    files = []
    while pending:
        parent = pending.pop()
        for path in parent.iterdir():
            count += 1
            if count > DEPENDENCY_FILES * 2 or linked(path):
                raise ValueError('Linked/oversized vendor tree')
            if path.is_dir():
                pending.append(path)
            elif path.is_file():
                files.append(path)
            else:
                raise ValueError('Special vendor entry')
    if len(files) > DEPENDENCY_FILES:
        raise ValueError('Too many dependency files')
    for path in sorted(files):
        raw = read_file(path, DEPENDENCY_MEMBER)
        total += len(raw)
        if total > DEPENDENCY_TOTAL:
            raise ValueError('Dependency sources exceed byte budget')
        yield 'vendor/' + path.relative_to(vendor).as_posix(), raw, 0o644


def build(repository, commit, vendor, output):
    repository = directory(repository)
    output = Path(output).absolute()
    directory(output.parent)
    output.mkdir(mode=0o700)
    commit = build_source(repository, commit, output / 'source.tar')
    selected = inspect_source(output / 'source.tar', commit)
    web = locked_web_packages(selected.get(WEB_LOCK))
    with tarfile.open(output / 'dependencies.tar', 'x', format=tarfile.USTAR_FORMAT, encoding='utf-8') as archive:
        add(archive, 'offline-build.txt', BUILD + (WEB_BUILD if web else b''))
        if web:
            with ThreadPoolExecutor(max_workers=6) as pool:
                for checksum, raw in zip(web, pool.map(download_web_package, web.values()), strict=True):
                    if hashlib.sha512(raw).hexdigest() != checksum:
                        raise ValueError('Downloaded browser package differs from its lock')
                    add(archive, web_path(checksum), raw)
        for path, raw, mode in vendor_entries(vendor):
            add(archive, path, raw, mode)
        for path, raw in sorted(selected.items()):
            if path.startswith('deploy/materials/licenses/'):
                add(archive, 'supplemental-licenses/' + path.rsplit('/', 1)[1], raw)
    notices = inspect_dependencies(tar_entries(output / 'dependencies.tar', DEPENDENCY_FILES, DEPENDENCY_MEMBER, DEPENDENCY_TOTAL), selected)
    (output / 'notices.json').write_bytes(encoded({'format': FORMAT, 'source_commit': commit, **notices}))
    return verify(output, commit)


def verify(materials, expected_commit):
    materials = directory(materials)
    if not re.fullmatch('[a-f0-9]{40}', expected_commit) or {p.name for p in materials.iterdir()} != {'source.tar', 'dependencies.tar', 'notices.json'}:
        raise ValueError('Invalid material set/source commit')
    selected = inspect_source(materials / 'source.tar', expected_commit)
    notices = inspect_dependencies(tar_entries(materials / 'dependencies.tar', DEPENDENCY_FILES, DEPENDENCY_MEMBER, DEPENDENCY_TOTAL), selected)
    expected = {'format': FORMAT, 'source_commit': expected_commit, **notices}
    if read_file(materials / 'notices.json', MAX_METADATA) != encoded(expected):
        raise ValueError('Notices do not match committed source/dependencies/licenses')
    report = {'format': FORMAT, 'source_commit': expected_commit, 'dependencies': len(notices['dependencies']), 'source_proof': 'git-objects-and-complete-blobs', 'sha256': {name: file_digest(materials / name) for name in ('source.tar', 'dependencies.tar', 'notices.json')}, 'installation_authority': False}
    if 'web_dependencies' in notices:
        report['web_dependencies'] = len(notices['web_dependencies'])
    return report


def extract(materials, expected_commit, output):
    # The material directory/ancestors remain operator-controlled for this operation.
    result = verify(materials, expected_commit)
    output = Path(output).absolute()
    directory(output.parent)
    output.mkdir(mode=0o700)
    marker = output / '.materials-incomplete'
    marker.write_bytes(b'verification/copy incomplete\n')
    def copy(name, raw, mode):
        target = output / name
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open('xb') as destination:
            destination.write(raw)
        target.chmod(mode)
    selected = inspect_source(Path(materials) / 'source.tar', expected_commit, copy)
    notices = inspect_dependencies(tar_entries(Path(materials) / 'dependencies.tar', DEPENDENCY_FILES, DEPENDENCY_MEMBER, DEPENDENCY_TOTAL), selected, copy)
    if read_file(Path(materials) / 'notices.json', MAX_METADATA) != encoded({'format': FORMAT, 'source_commit': expected_commit, **notices}):
        raise ValueError('Notices changed during material extraction')
    marker.unlink()
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    prepare = commands.add_parser('build')
    prepare.add_argument('--repository', type=Path, default=Path(__file__).resolve().parents[1])
    prepare.add_argument('--commit', default='HEAD')
    prepare.add_argument('--vendor', type=Path, required=True)
    prepare.add_argument('--output', type=Path, required=True)
    for name in ('verify', 'extract'):
        check = commands.add_parser(name)
        check.add_argument('--materials', type=Path, required=True)
        check.add_argument('--source-commit', required=True)
        if name == 'extract':
            check.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if args.command == 'build':
        result = build(args.repository, args.commit, args.vendor, args.output)
    elif args.command == 'extract':
        result = extract(args.materials, args.source_commit, args.output)
    else:
        result = verify(args.materials, args.source_commit)
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
