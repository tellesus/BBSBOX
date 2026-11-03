# SPDX-License-Identifier: AGPL-3.0-or-later
from datetime import datetime, timedelta, timezone
import hashlib
import os
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "deploy"))
from backup_retention import Receipt, keep, prune
from backup_schedule import mount_entries, run_locked


def record(number, at):
    data = f"archive-{number}".encode()
    return Receipt(
        f"backup-{number:032x}.age",
        int(at.timestamp()),
        "ab" * 32,
        hashlib.sha256(data).hexdigest(),
        len(data),
    ), data


class RetentionPlan(unittest.TestCase):
    def test_mount_lookup_matches_whole_decoded_mountpoint(self):
        table = (
            "42 1 8:2 / /media/My\\040Drive rw,nosuid shared:5 - ext4 /dev/sda2 rw\n"
        )
        self.assertEqual(
            mount_entries(table, "/media/My Drive"),
            [("42", "8:2", "ext4", ["rw", "nosuid"])],
        )
        self.assertEqual(mount_entries(table, "/media/My"), [])

    def test_latest_daily_and_weekly_points_survive_a_year_boundary_and_duplicate_day(
        self,
    ):
        newest = datetime(2026, 1, 20, 5, tzinfo=timezone.utc)
        entries = [record(i, newest - timedelta(days=i))[0] for i in range(36)]
        later = record(99, newest + timedelta(hours=2))[0]
        self.assertEqual(
            keep([*reversed(entries), later]),
            {later.name, *[entries[i].name for i in [1, 2, 3, 4, 5, 6, 9, 16]]},
        )


@unittest.skipUnless(sys.platform == "linux", "Anchored Linux media operations")
class MediaPruning(unittest.TestCase):
    def test_unmounted_path_is_not_used_for_backup_or_retention(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary)
            marker = path / "existing.age"
            marker.write_bytes(b"retain")
            self.assertEqual(
                run_locked({"mount": str(path)}), {"status": "media-unavailable"}
            )
            self.assertEqual(list(path.iterdir()), [marker])
            self.assertEqual(marker.read_bytes(), b"retain")

    def test_pruning_preserves_corrupt_linked_and_unindexed_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            media = root / "media"
            media.mkdir(mode=0o700)
            newest = datetime(2026, 1, 20, 5, tzinfo=timezone.utc)
            records = []
            for i in range(36):
                receipt, data = record(i, newest - timedelta(days=i))
                records.append(receipt)
                target = media / receipt.name
                target.write_bytes(data)
                target.chmod(0o600)
            corrupted = media / records[30].name
            corrupted.write_bytes(b"x" * records[30].size)
            outside = root / "other.age"
            outside.write_bytes(b"outside")
            outside.chmod(0o600)
            linked = media / records[32].name
            linked.unlink()
            linked.symlink_to(outside)
            unknown = media / record(99, newest)[0].name
            unknown.write_bytes(b"unindexed")
            directory = os.open(media, os.O_RDONLY | os.O_DIRECTORY)
            try:
                report = prune(directory, records, owner=os.getuid())
            finally:
                os.close(directory)
            self.assertIn(records[35].name, report["pruned"])
            self.assertFalse((media / records[35].name).exists())
            self.assertEqual(set(report["unverified"]), {corrupted.name, linked.name})
            self.assertTrue(all((media / name).is_file() for name in report["kept"]))
            self.assertEqual(unknown.read_bytes(), b"unindexed")
            self.assertTrue(linked.is_symlink())
            self.assertEqual(outside.read_bytes(), b"outside")
            self.assertEqual(corrupted.read_bytes(), b"x" * records[30].size)

    def test_sole_intact_older_archive_is_kept_when_recent_copies_are_damaged(self):
        with tempfile.TemporaryDirectory() as temporary:
            media = Path(temporary)
            now = datetime(2026, 1, 20, tzinfo=timezone.utc)
            receipts = []
            for i in range(20):
                receipt, data = record(i, now - timedelta(days=i))
                receipts.append(receipt)
                path = media / receipt.name
                path.write_bytes(data if i == 19 else b"x" * len(data))
                path.chmod(0o600)
            directory = os.open(media, os.O_RDONLY | os.O_DIRECTORY)
            try:
                report = prune(directory, receipts, owner=os.getuid())
            finally:
                os.close(directory)
            self.assertEqual(report["pruned"], [])
            self.assertEqual(report["kept"], [receipts[-1].name])
            self.assertEqual(len(list(media.iterdir())), 20)


if __name__ == "__main__":
    unittest.main()
