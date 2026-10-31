# SPDX-License-Identifier: AGPL-3.0-or-later
"""Integrity and target checks for the offline development payload; not release signature evidence."""
import json
from pathlib import Path
import tempfile
import unittest
import runtime_bundle as bundle


class BundleTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name).resolve()
        self.binary = self.root/'fixture-elf'
        header = bytearray(64)
        header[:7] = b'\x7fELF\x02\x01\x01'
        header[16:20] = b'\x03\0\x3e\0'
        self.binary.write_bytes(header+b'fixture; not a runnable program')
        self.output = self.root/'payload'

    def tearDown(self):
        assert self.output.resolve().is_relative_to(self.root)
        self.temporary.cleanup()

    def build(self):
        return bundle.build(self.binary, self.output, 'a'*40, True)

    def test_complete_payload_round_trips_and_preserves_source_record(self):
        result = self.build()
        self.assertEqual(result, bundle.verify(self.output))
        self.assertTrue(result['source_dirty'])
        self.assertEqual(result['files'], 6)

    def test_windows_or_wrong_architecture_binary_is_rejected_before_creating_output(self):
        arm=bytearray(self.binary.read_bytes());arm[18:20]=b'\xb7\0'
        foreign=bytearray(self.binary.read_bytes());foreign[7]=9
        for raw in [b'MZ'+bytes(100), b'not an executable',arm,foreign]:
            self.binary.write_bytes(raw)
            with self.assertRaises(ValueError):
                self.build()
            self.assertFalse(self.output.exists())

    def test_existing_destination_is_never_overwritten(self):
        first = self.build()
        with self.assertRaises(FileExistsError):
            self.build()
        self.assertEqual(bundle.verify(self.output), first)

    def test_corrupt_missing_or_unlisted_files_cannot_verify(self):
        self.build()
        member = self.output/'bbsbox-node'
        original = member.read_bytes()
        member.write_bytes(original+b'changed')
        with self.assertRaises(ValueError):
            bundle.verify(self.output)
        member.write_bytes(original)
        extra = self.output/'unlisted'
        extra.write_bytes(b'x')
        with self.assertRaises(ValueError):
            bundle.verify(self.output)
        extra.unlink()
        member.unlink()
        with self.assertRaises((ValueError, OSError)):
            bundle.verify(self.output)

    def test_path_escape_duplicate_and_unknown_profiles_are_rejected_without_writes(self):
        self.build()
        path = self.output/'manifest.json'
        original = json.loads(path.read_bytes())
        import copy
        for change in [lambda m:m['files'][0].update(path='../escaped'), lambda m:m['files'].__setitem__(1, m['files'][0]), lambda m:m.update(format='bbsbox.release/v1')]:
            manifest = copy.deepcopy(original)
            change(manifest)
            path.write_bytes(bundle.encoded(manifest))
            with self.assertRaises(ValueError):
                bundle.verify(self.output)
            self.assertFalse((self.root/'escaped').exists())

    def test_symlinked_member_is_rejected(self):
        self.build()
        member = self.output/'bbsbox-node'
        member.unlink()
        try:
            member.symlink_to(self.binary)
        except OSError:
            self.skipTest('Symlink creation is unavailable in this Windows session')
        with self.assertRaises(ValueError):
            bundle.verify(self.output)


if __name__ == '__main__':
    unittest.main()
