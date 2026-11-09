# SPDX-License-Identifier: AGPL-3.0-or-later
"""Prepare independently verifiable Windows client sources/dependencies; never sign or install."""
import argparse
from pathlib import Path
import tarfile
import release_materials as materials

FORMAT = 'bbsbox.client-materials/v1'
NATIVE_LOCK = 'spikes/m00/native/Cargo.lock'
SUPPLEMENTS = 'deploy/client-materials/supplements.json'
LICENSES = 'deploy/client-materials/licenses/'
BUILD = b'''Windows x86-64 client build. Prepare the pinned Rust 1.99.1 MSVC toolchain,
Microsoft C++ Build Tools/Windows SDK and WebView2 separately.
Extract source.tar and dependencies.tar into one fresh directory. From source/:
cargo --config 'source.crates-io.replace-with="vendored-sources"' --config 'source.vendored-sources.directory="../vendor"' build --locked --offline -p bbsbox-maintenance --example client_package
cargo --config 'source.crates-io.replace-with="vendored-sources"' --config 'source.vendored-sources.directory="../vendor"' build --locked --offline --manifest-path spikes/m00/native/Cargo.toml
The native UI is bundled directly; its build requires no npm download.
Release trust enrollment and signing are separate operations. No release private key belongs in these materials.
'''


def selected(path, commit, consume=None):
    licenses = {}

    def visit(name, raw, mode):
        if name.startswith('source/' + LICENSES):
            licenses[name[len('source/' + LICENSES):]] = raw
        if consume:
            consume(name, raw, mode)

    result = materials.inspect_source(path, commit, visit, extra_paths=(NATIVE_LOCK, SUPPLEMENTS))
    if NATIVE_LOCK not in result:
        raise ValueError('Missing native client lockfile')
    # The native UI is embedded HTML/JS/CSS. Node browser-shell npm packages are not build inputs.
    result.pop(materials.WEB_LOCK, None)
    result.pop(materials.WEB_NOTICE, None)
    if SUPPLEMENTS in result:
        extra = materials.decoded(result[SUPPLEMENTS])
        original = materials.decoded(result['deploy/materials/supplements.json'])
        if set(extra) != {'format', 'files', 'packages'} or extra['format'] != original['format']:
            raise ValueError('Invalid client license supplements')
        for section in ('files', 'packages'):
            if set(original[section]) & set(extra[section]):
                raise ValueError('Conflicting client license supplements')
            original[section].update(extra[section])
        for name, raw in licenses.items():
            if '/' in name:
                raise ValueError('Invalid client license filename')
            result['deploy/materials/licenses/' + name] = raw
        result['deploy/materials/supplements.json'] = materials.encoded(original)
    return result


def inspect(path, source, consume=None):
    return materials.inspect_dependencies(
        materials.tar_entries(path, materials.DEPENDENCY_FILES, materials.DEPENDENCY_MEMBER, materials.DEPENDENCY_TOTAL),
        source, consume, extra_locks=(source[NATIVE_LOCK],), instructions=BUILD,
    )


def verify(directory, commit):
    directory = materials.directory(directory)
    if {p.name for p in directory.iterdir()} != {'source.tar', 'dependencies.tar', 'notices.json'}:
        raise ValueError('Invalid client material set')
    source = selected(directory / 'source.tar', commit)
    notices = inspect(directory / 'dependencies.tar', source)
    expected = dict(format=FORMAT, source_commit=commit, target='windows-x86_64-msvc', **notices)
    if materials.read_file(directory / 'notices.json', materials.MAX_METADATA) != materials.encoded(expected):
        raise ValueError('Client notices differ from source/dependency evidence')
    return dict(format=FORMAT, source_commit=commit, dependencies=len(notices['dependencies']),
                sha256={name: materials.file_digest(directory / name) for name in ('source.tar', 'dependencies.tar', 'notices.json')},
                source_proof='git-objects-and-complete-blobs', installation_authority=False)


def build(repository, commit, vendor, output):
    repository = materials.directory(repository)
    output = Path(output).absolute()
    materials.directory(output.parent)
    output.mkdir(mode=0o700)
    commit = materials.build_source(repository, commit, output / 'source.tar')
    source = selected(output / 'source.tar', commit)
    with tarfile.open(output / 'dependencies.tar', 'x', format=tarfile.USTAR_FORMAT, encoding='utf-8') as archive:
        materials.add(archive, 'offline-build.txt', BUILD)
        for name, raw, mode in materials.vendor_entries(vendor):
            materials.add(archive, name, raw, mode)
        for name, raw in sorted(source.items()):
            if name.startswith('deploy/materials/licenses/'):
                materials.add(archive, 'supplemental-licenses/' + name.rsplit('/', 1)[1], raw)
    notices = inspect(output / 'dependencies.tar', source)
    (output / 'notices.json').write_bytes(materials.encoded(dict(format=FORMAT, source_commit=commit, target='windows-x86_64-msvc', **notices)))
    return verify(output, commit)


def extract(directory, commit, output):
    directory = materials.directory(directory)
    proof = verify(directory, commit)
    output = Path(output).absolute()
    materials.directory(output.parent)
    output.mkdir(mode=0o700)

    def write(name, raw, mode):
        path = output / name
        path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        with path.open('xb') as destination:
            destination.write(raw)
        path.chmod(mode)

    source = selected(directory / 'source.tar', commit, write)
    inspect(directory / 'dependencies.tar', source, write)
    return proof


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    create = commands.add_parser('build')
    create.add_argument('--repository', type=Path, default=Path(__file__).resolve().parents[1])
    create.add_argument('--commit', required=True)
    create.add_argument('--vendor', type=Path, required=True)
    create.add_argument('--output', type=Path, required=True)
    for name in ('verify', 'extract'):
        command = commands.add_parser(name)
        command.add_argument('materials', type=Path)
        command.add_argument('--commit', required=True)
        if name == 'extract':
            command.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if args.command == 'build':
        result = build(args.repository, args.commit, args.vendor, args.output)
    elif args.command == 'extract':
        result = extract(args.materials, args.commit, args.output)
    else:
        result = verify(args.materials, args.commit)
    print(materials.encoded(result).decode(), end='')
