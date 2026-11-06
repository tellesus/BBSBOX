# SPDX-License-Identifier: AGPL-3.0-or-later
"""The exported replacement fixture must contain the actual maintenance entry points."""
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'deploy/ci'))
import recover_service


class RecoveryContext(unittest.TestCase):
    def test_game_replacement_contains_working_backup_update_and_rollback_commands(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)/'guest';fixture=Path(temporary)/'fixture'
            for directory in ['src/deploy','src/tools','root']:
                (root/directory).mkdir(parents=True,exist_ok=True)
            (fixture/'game_update').mkdir(parents=True)
            (fixture/'game_update/manifest.cose').write_bytes(b'public fixture')
            recover_service.game_maintenance_context(root,fixture)
            for command in ['backup_node.py','update_release.py','rollback_release.py']:
                result=subprocess.run([sys.executable,'-E','-s',str(root/'src/deploy'/command),'--help'],
                                      capture_output=True,timeout=10)
                self.assertEqual(result.returncode,0,result.stderr.decode(errors='replace'))
            self.assertEqual((root/'root/bbsbox-ci-game-update/manifest.cose').read_bytes(),b'public fixture')
            self.assertEqual((root/'usr/local/lib/bbsbox-game-worker-probe.py').read_bytes(),
                             (ROOT/'deploy/ci/game_worker_case.py').read_bytes())


if __name__=='__main__':unittest.main()
