# SPDX-License-Identifier: AGPL-3.0-or-later
"""Read-only first-install guards; actual Debian setup/service checks run in a disposable Linux target."""
from pathlib import Path
import sys
import tempfile
import unittest
import importlib.util
import tarfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1]/'deploy'))
import install_development as installer
spec=importlib.util.spec_from_file_location('ci_prepare',Path(__file__).resolve().parents[1]/'deploy/ci/prepare.py')
prepare=importlib.util.module_from_spec(spec);spec.loader.exec_module(prepare)


class FirstInstallTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name).resolve()
        self.layout = installer.Layout(self.root)

    def tearDown(self):
        assert self.layout.data.resolve().is_relative_to(self.root)
        self.temporary.cleanup()

    def test_fresh_layout_guard_does_not_create_any_files(self):
        installer.check_fresh(self.layout)
        self.assertEqual(list(self.root.iterdir()), [])

    def test_existing_configuration_unit_or_activation_is_preserved_and_rejected(self):
        for target in [self.layout.config/'node.json', self.layout.unit, self.layout.application/'current']:
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(b'existing user state')
            with self.assertRaises(ValueError):
                installer.check_fresh(self.layout)
            self.assertEqual(target.read_bytes(), b'existing user state')
            target.unlink()

    def test_nonempty_data_directory_is_preserved_and_rejected(self):
        self.layout.data.mkdir(parents=True)
        source = self.layout.data/'existing.sqlite'
        source.write_bytes(b'existing community')
        with self.assertRaises(ValueError):
            installer.check_fresh(self.layout)
        self.assertEqual(source.read_bytes(), b'existing community')

    def test_tls_material_is_never_replaced_by_first_install(self):
        tls = self.layout.config/'tls'
        tls.mkdir(parents=True)
        with self.assertRaises(ValueError):
            installer.check_fresh(self.layout)
        self.assertTrue(tls.is_dir())


class ArchiveTests(unittest.TestCase):
    def test_os_symlinks_are_allowed_without_extraction_through_them(self):
        directory=tarfile.TarInfo('usr');directory.type=tarfile.DIRTYPE
        binary=tarfile.TarInfo('usr/program')
        link=tarfile.TarInfo('program');link.type=tarfile.SYMTYPE;link.linkname='/usr/program'
        prepare.validate_archive([directory,binary,link])

    def test_escaping_names_links_duplicates_and_device_records_are_rejected_before_extraction(self):
        absolute=tarfile.TarInfo('/etc/escape')
        parent=tarfile.TarInfo('../escape')
        symlink=tarfile.TarInfo('through');symlink.type=tarfile.SYMTYPE;symlink.linkname='/outside'
        child=tarfile.TarInfo('through/escape')
        hardlink=tarfile.TarInfo('hardlink');hardlink.type=tarfile.LNKTYPE;hardlink.linkname='../../escape'
        device=tarfile.TarInfo('device');device.type=tarfile.CHRTYPE
        for members in [[absolute],[parent],[symlink,child],[hardlink],[device],[child,child]]:
            with self.subTest(members=members),self.assertRaises(ValueError):prepare.validate_archive(members)


if __name__ == '__main__':
    unittest.main()
