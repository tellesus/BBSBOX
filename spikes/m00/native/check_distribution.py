# SPDX-License-Identifier: AGPL-3.0-or-later
"""Exercise a real signed Windows client through a separately pinned local bootstrap."""
import argparse
from contextlib import redirect_stdout
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import uuid
from check import check, ROOT

sys.path.insert(0, str(ROOT / 'tools'))
import client_materials


def distribution(binary, bootstrap, materials, commit):
    if os.name != 'nt':
        raise RuntimeError('Actual installed-client execution requires Windows')
    binary = binary.resolve(strict=True)
    bootstrap = bootstrap.resolve(strict=True)
    materials = materials.resolve(strict=True)
    material_proof = client_materials.verify(materials, commit)
    (ROOT / 'work').mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='signed-client-', dir=ROOT / 'work') as temporary:
        root = Path(temporary)
        package = root / 'package'
        public_file = root / 'fixture-authority.public'
        store = root / 'installed'

        def tool(*arguments, error=None):
            result = subprocess.run([str(bootstrap), *map(str, arguments)], capture_output=True,
                                    timeout=240, creationflags=subprocess.CREATE_NO_WINDOW)
            message = result.stderr.decode(errors='replace')
            if error is not None:
                assert result.returncode != 0 and error in message, message
                return None
            if result.returncode:
                raise RuntimeError(message[-4096:])
            return json.loads(result.stdout)

        issued = tool('fixture-package', binary, ROOT, materials, commit, package, public_file)
        public = public_file.read_text(encoding='ascii')
        assert issued['production_signing'] is False and issued['public_key'] == public
        tool('verify', package, public)
        other_public = json.loads((ROOT / 'protocol/fixtures/release-manifest-v1.json').read_text())['public_hex']
        tool('verify', package, other_public, error='expected release authority')
        tool('init', store, public)
        request = uuid.uuid4().hex
        pending = tool('prepare', store, request, package)
        assert pending['active'] is None and pending['pending'] is not None
        tool('run', store, '--', '--check', root / 'premature.json', '--request', root / 'absent.json',
             error='finish the pending client installation')
        assert tool('status', store) == pending  # separate processes reopen the durable pending state
        tool('activate', store, '02' * 16, error='no matching client installation')
        active = tool('activate', store, request)
        assert active['pending'] is None and active['highest_sequence'] == 1
        installed = Path(tool('path', store)['executable'])
        assert installed.resolve().is_relative_to(store.resolve())
        with redirect_stdout(io.StringIO()) as captured:
            check(bootstrap, store)
        native = json.loads(captured.getvalue())
        # Alter an installed executable after acceptance. Launch rechecks the complete inventory.
        with installed.open('r+b') as target:
            original = target.read(1)
            target.seek(0)
            target.write(b'X')
        tool('run', store, '--', '--check', root / 'tampered.json', '--request', root / 'absent.json',
             error='signed commitment')
        assert not (root / 'tampered.json').exists()
        with installed.open('r+b') as target:
            target.write(original)
        tool('path', store)
        assert tool('prepare', store, request, package) == active
        (package / 'LICENSE.md').write_bytes(b'altered transport payload')
        tool('verify', package, public, error='signed commitment')
        tool('path', store)  # the installed copy is independent of later transport changes
        return dict(format='bbsbox.signed-native-client-check/v1', source_commit=commit,
                    materials=material_proof, production_signing=False,
                    independent_fixture_authority=True, wrong_authority_rejected=True,
                    interrupted_install_resumed=True, changed_installed_bytes_rejected=True,
                    independent_staged_copy=True, native=native)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--bootstrap', type=Path, required=True)
    parser.add_argument('--materials', type=Path, required=True)
    parser.add_argument('--commit', required=True)
    args = parser.parse_args()
    print(json.dumps(distribution(args.binary, args.bootstrap, args.materials, args.commit), indent=2))
