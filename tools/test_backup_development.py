# SPDX-License-Identifier: AGPL-3.0-or-later
import os
from pathlib import Path
import stat
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'deploy'))
import backup_development as backup


class BackupGuards(unittest.TestCase):
    def test_disabled_game_with_a_retained_save_still_requires_the_game_profile(self):
        with tempfile.TemporaryDirectory() as directory:
            game=Path(directory)/'game'
            with patch.object(backup,'GAME_STATE',game):
                self.assertFalse(backup.game_state_retained({}))
                self.assertTrue(backup.game_state_retained(dict(game_origin='https://game.home.arpa')))
                game.mkdir();(game/'state.sqlite.lock').touch()
                self.assertFalse(backup.game_state_retained({}))
                (game/'state.sqlite').write_bytes(b'save')
                self.assertTrue(backup.game_state_retained({}))
                (game/'state.sqlite').unlink();(game/'state.sqlite.lock').unlink();game.rmdir()
                game.write_bytes(b'unexpected file')
                with self.assertRaises(ValueError):backup.game_state_retained({})

    def test_temporary_checkpoint_socket_stops_even_when_backup_fails(self):
        for status in [b'inactive',b'active']:
            with self.subTest(status=status),patch.object(backup.subprocess,'run',return_value=SimpleNamespace(stdout=status)),patch.object(backup,'run') as run:
                with self.assertRaisesRegex(RuntimeError,'backup failed'):
                    with backup.checkpoint_socket(True):raise RuntimeError('backup failed')
                actions=[call.args[0][1] for call in run.call_args_list]
                self.assertEqual(actions,['start','stop'] if status==b'inactive' else [])
        with patch.object(backup.subprocess,'run',return_value=SimpleNamespace(stdout=b'failed')),patch.object(backup,'run') as run:
            with self.assertRaises(ValueError):
                with backup.checkpoint_socket(True):self.fail('Invalid socket state was admitted')
            run.assert_not_called()

    def test_unprivileged_invocation_cannot_start_maintenance(self):
        with patch.object(backup.os,'geteuid',return_value=1000,create=True),patch.object(backup.subprocess,'run') as process:
            with self.assertRaises(ValueError):backup.backup('/unused.age','age1fixture')
            process.assert_not_called()

    @unittest.skipUnless(sys.platform=='linux','Unix descriptor ownership/permission checks')
    def test_owned_sidecars_become_private_without_touching_other_files(self):
        with tempfile.TemporaryDirectory() as directory:
            database=Path(directory)/'node.sqlite'
            for suffix in ['', '-wal', '-shm', '-journal', '.writer.lock']:
                path=Path(str(database)+suffix);path.write_bytes(b'kept');path.chmod(0o644)
            other=Path(directory)/'unrelated';other.write_bytes(b'original');other.chmod(0o644)
            backup.repair_ownership(database,os.getuid(),os.getgid())
            for suffix in ['', '-wal', '-shm', '-journal', '.writer.lock']:
                path=Path(str(database)+suffix)
                self.assertEqual(path.read_bytes(),b'kept');self.assertEqual(stat.S_IMODE(path.stat().st_mode),0o600)
            self.assertEqual(stat.S_IMODE(other.stat().st_mode),0o644)

    @unittest.skipUnless(sys.platform=='linux','Unix nonsymlink descriptor checks')
    def test_symlinked_sidecar_cannot_change_an_external_file(self):
        with tempfile.TemporaryDirectory() as directory:
            database=Path(directory)/'node.sqlite';database.write_bytes(b'node')
            other=Path(directory)/'outside';other.write_bytes(b'original');other.chmod(0o644)
            Path(str(database)+'-wal').symlink_to(other)
            with self.assertRaises(OSError):backup.repair_ownership(database,os.getuid(),os.getgid())
            self.assertEqual(other.read_bytes(),b'original');self.assertEqual(stat.S_IMODE(other.stat().st_mode),0o644)


if __name__=='__main__':unittest.main()
