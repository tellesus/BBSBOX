# SPDX-License-Identifier: AGPL-3.0-or-later
"""Rebuild extracted, verified release materials inside a separately created empty network namespace."""
import argparse
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[2]/'tools'))
import release_materials as materials


def verify_web_build(root, node, npm, environment):
    shell = root / 'source/web/shell'
    if node is None or npm is None:
        raise ValueError('Browser material rebuild needs prepared Node.js and npm')
    node, npm = node.resolve(strict=True), npm.absolute()
    environment = dict(environment, PATH=str(node.parent) + os.pathsep + environment['PATH'])
    version = subprocess.check_output([str(node), '--version'], env=environment, text=True).strip()
    if version != 'v' + (shell / '.node-version').read_text().strip():
        raise ValueError('Browser rebuild Node.js version differs from the pinned tool')
    options = ['--offline', '--cache', str(root / 'empty-npm-cache'), '--ignore-scripts', '--no-audit', '--no-fund']
    for command in [
        ['cache', 'add', *options, *map(str, sorted((root / 'npm-packages').glob('*.tgz')))],
        ['ci', *options],
        ['run', 'verify'],
    ]:
        result = subprocess.run([str(npm), *command], cwd=shell, env=environment, capture_output=True, text=True, timeout=180)
        if result.returncode:
            raise RuntimeError('Offline browser rebuild failed:\n' + (result.stdout + result.stderr)[-8000:])
    return version


def rebuild(root, material_directory, source_commit, cargo, rustc, expected_profile, output, node=None, npm=None):
    if sys.platform != 'linux' or os.geteuid() != 0 or {name for _, name in socket.if_nameindex()} != {'lo'} or os.readlink('/proc/self/ns/net') == os.readlink('/proc/1/ns/net'):
        raise ValueError('Rebuild fixture requires its own root-owned loopback-only network namespace')
    # Verify/copy into a fresh private root in this namespace before compiling those exact inputs.
    material_report = materials.extract(material_directory, source_commit, root)
    root = root.resolve(strict=True)
    cargo = cargo.resolve(strict=True)
    rustc = rustc.resolve(strict=True)
    if not (root/'source/Cargo.lock').is_file() or not (root/'vendor').is_dir() or (root/'.materials-incomplete').exists():
        raise ValueError('Missing verified source/vendor extraction')
    cache = root/'empty-cargo-home'
    cache.mkdir(mode=0o700)
    target = root/'rebuild-target'
    if target.exists():
        raise ValueError('Rebuild must use a fresh target directory')
    environment = dict(os.environ, CARGO_HOME=str(cache), CARGO_TARGET_DIR=str(target), RUSTC=str(rustc), CARGO_PROFILE_DEV_DEBUG='0', CARGO_INCREMENTAL='0')
    # Use direct pinned compiler binaries; changing Cargo's empty home must not trigger rustup fetches.
    environment['PATH'] = str(rustc.parent) + os.pathsep + environment.get('PATH', '')
    version = subprocess.check_output([str(rustc), '--version'], env=environment, text=True).strip()
    if not version.startswith('rustc 1.99.1 '):
        raise ValueError('Rebuild compiler differs from the pinned version')
    web_node = verify_web_build(root, node, npm, environment) if material_report.get('web_dependencies') else None
    command = [str(cargo), '--config', 'source.crates-io.replace-with="vendored-sources"', '--config', 'source.vendored-sources.directory='+json.dumps(str(root/'vendor')), 'build', '--frozen', '-p', 'bbsbox-node-runtime', '-p', 'bbsbox-demo-worker', '--bins', '--examples']
    started = time.monotonic()
    result = subprocess.run(command, cwd=root/'source', env=environment, capture_output=True, text=True, timeout=600)
    if result.returncode:
        raise RuntimeError('Offline material rebuild failed:\n'+result.stderr[-8000:])
    binary = target/'debug/bbsbox-node'
    profile = subprocess.run([str(binary), '--release-profile'], capture_output=True, text=True, check=True, timeout=20)
    information = json.loads(profile.stdout)
    with expected_profile.open('rb') as source:
        expected = source.read(65537)
    if len(expected) > 65536 or information != json.loads(expected) or information.get('format') != 'bbsbox.release-runtime/v1' or information.get('target') != 'debian13-amd64':
        raise ValueError('Rebuilt executable does not report the expected target/profile')
    report = dict(format='bbsbox.offline-material-rebuild/v1', compiler=version, elapsed_seconds=round(time.monotonic()-started, 2), source_commit=source_commit, materials_sha256=material_report['sha256'], binary_sha256=materials.file_digest(binary), empty_cargo_home=True, empty_target=True, network='isolated-loopback-only', compiled_profile_matches_candidate=True, compiled_profile=information)
    report['demo_worker_sha256'] = materials.file_digest(binary.with_name('bbsbox-demo-worker'))
    if web_node:
        report.update(browser_templates_reproduced=True, empty_npm_cache=True, node=web_node)
    # Real first-party assemblies with distinct compiled SQL, built from the same committed materials.
    # Each gets its own bound report; never substitute a fake profile or binary digest.
    for name in ['upgrade_successor','upgrade_rejecting','upgrade_interrupted']:
        executable=target/'debug/examples'/name
        compiled=json.loads(subprocess.check_output([str(executable),'--release-profile'],timeout=20))
        if compiled['module_migrations']==information['module_migrations'] or compiled['target']!='debian13-amd64':
            raise ValueError('Upgrade fixture lacks its distinct actual compiled registry')
        evidence=dict(report,binary_sha256=materials.file_digest(executable),compiled_profile=compiled,fixture=name)
        with (output.parent/('offline-'+name+'.json')).open('x',encoding='utf-8') as destination:
            json.dump(evidence,destination,indent=2);destination.write('\n')
    with output.open('x', encoding='utf-8') as destination:
        json.dump(report, destination, indent=2)
        destination.write('\n')
    return report


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root',type=Path,required=True)
    parser.add_argument('--materials',type=Path,required=True)
    parser.add_argument('--source-commit',required=True)
    parser.add_argument('--cargo',type=Path,required=True)
    parser.add_argument('--rustc',type=Path,required=True)
    parser.add_argument('--node',type=Path)
    parser.add_argument('--npm',type=Path)
    parser.add_argument('--expected-profile',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    print(json.dumps(rebuild(args.root,args.materials,args.source_commit,args.cargo,args.rustc,args.expected_profile,args.output,args.node,args.npm),indent=2))


if __name__=='__main__': main()
