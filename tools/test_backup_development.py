# SPDX-License-Identifier: AGPL-3.0-or-later
import os
from pathlib import Path
import stat
import sys
import tempfile
import unittest
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'deploy'))
import backup_development as backup


class BackupGuards(unittest.TestCase):
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
