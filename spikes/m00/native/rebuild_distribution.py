# SPDX-License-Identifier: AGPL-3.0-or-later
"""Rebuild verified client sources with a fresh Cargo home/target and test identity continuity."""
import argparse
from contextlib import redirect_stdout
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
from check import check, ROOT
from check_distribution import distribution

sys.path.insert(0, str(ROOT / 'tools'))
import client_materials


def rebuild(materials, commit, cargo, rustc, original):
    if os.name != 'nt':
        raise RuntimeError('This native build proof requires Windows/MSVC tools')
    cargo = cargo.resolve(strict=True)
    rustc = rustc.resolve(strict=True)
    original = original.resolve(strict=True)
    (ROOT / 'work').mkdir(exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix='offline-client-', dir=ROOT / 'work'))
    extracted = directory / 'extracted'
    material_proof = client_materials.extract(materials, commit, extracted)
    source = extracted / 'source'
    cache = directory / 'empty-cargo-home'
    cache.mkdir()
    target = directory / 'target'
    environment = dict(os.environ, CARGO_HOME=str(cache), RUSTC=str(rustc), CARGO_TARGET_DIR=str(target))
    config = ['--config', 'source.crates-io.replace-with="vendored-sources"', '--config',
              'source.vendored-sources.directory=' + json.dumps(str(extracted / 'vendor'))]
    elapsed = {}
    for name, arguments in (
        ('bootstrap', ['-p', 'bbsbox-maintenance', '--example', 'client_package']),
        ('native', ['--manifest-path', 'spikes/m00/native/Cargo.toml']),
    ):
        started = time.monotonic()
        with (directory / (name + '-build.log')).open('wb') as log:
            result = subprocess.run([str(cargo), *config, 'build', '--frozen', '--offline', *arguments],
                                    cwd=source, env=environment, stdout=log, stderr=subprocess.STDOUT, timeout=1200,
                                    creationflags=subprocess.CREATE_NO_WINDOW)
        if result.returncode:
            raise RuntimeError(f'Offline {name} build failed; inspect {directory / (name + "-build.log")}')
        elapsed[name] = round(time.monotonic() - started, 3)
    native = target / 'debug/bbsbox-native-probe.exe'
    bootstrap = target / 'debug/examples/client_package.exe'
    with redirect_stdout(io.StringIO()) as captured:
        check(original, restart_binary=native)
    continuity = json.loads(captured.getvalue())
    (directory / 'identity-continuity.json').write_text(json.dumps(continuity, indent=2), encoding='utf-8')
    (directory / 'build-evidence.json').write_text(json.dumps(dict(source_commit=commit, build_seconds=elapsed,
        cargo_frozen_offline=True, initially_empty_cargo_home=True, fresh_target=True), indent=2), encoding='utf-8')
    installed = distribution(native, bootstrap, materials, commit)
    # Build products/logs are retained under work for inspection; no user profile is opened.
    return dict(format='bbsbox.offline-native-rebuild/v1', source_commit=commit, materials=material_proof,
                cargo_frozen_offline=True, dependency_source='verified-extracted-vendor',
                initially_empty_cargo_home=True, fresh_target=True, build_seconds=elapsed,
                original_to_rebuilt_identity=continuity, signed_installed_client=installed,
                evidence_directory=str(directory))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--materials', type=Path, required=True)
    parser.add_argument('--commit', required=True)
    parser.add_argument('--cargo', type=Path, required=True)
    parser.add_argument('--rustc', type=Path, required=True)
    parser.add_argument('--original-binary', type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(rebuild(args.materials, args.commit, args.cargo, args.rustc, args.original_binary), indent=2))
