# SPDX-License-Identifier: AGPL-3.0-or-later
"""Portable signed first-install preservation guards; actual installation/boot runs in Debian CI."""
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'deploy'))
import install_release as release


class SignedInstallGuards(unittest.TestCase):
    def test_fresh_layout_is_read_only_and_existing_control_or_application_is_preserved(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();layout=release.base.Layout(root)
            release.check_fresh(layout);self.assertEqual(list(root.iterdir()),[])
            control=root/'var/lib/bbsbox-maintenance/release-control';control.mkdir(parents=True)
            marker=control/'highest';marker.write_bytes(b'never reset')
            with self.assertRaises(ValueError):release.check_fresh(layout)
            self.assertEqual(marker.read_bytes(),b'never reset')
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();layout=release.base.Layout(root);layout.application.mkdir(parents=True)
            marker=layout.application/'existing';marker.write_bytes(b'previous application')
            with self.assertRaises(ValueError):release.check_fresh(layout)
            self.assertEqual(marker.read_bytes(),b'previous application')

    def test_existing_node_data_is_preserved_before_bootstrap(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();layout=release.base.Layout(root);layout.data.mkdir(parents=True)
            marker=layout.data/'node.sqlite';marker.write_bytes(b'community data')
            with self.assertRaises(ValueError):release.check_fresh(layout)
            self.assertEqual(marker.read_bytes(),b'community data')

    def test_lost_control_does_not_make_a_previous_installation_or_restore_ticket_fresh(self):
        for name in ['first-install.json','first-restore.json']:
            with self.subTest(name=name),tempfile.TemporaryDirectory() as temporary:
                root=Path(temporary).resolve();layout=release.base.Layout(root)
                directory=root/'var/lib/bbsbox-maintenance';directory.mkdir(parents=True)
                ticket=directory/name;ticket.write_bytes(b'existing operator evidence')
                with self.assertRaises(ValueError):release.check_fresh(layout)
                self.assertEqual(ticket.read_bytes(),b'existing operator evidence')
                self.assertEqual(list(directory.iterdir()),[ticket])

    def test_partial_configuration_or_service_override_is_preserved(self):
        for name in ['etc/bbsbox/installation.json','etc/systemd/system/bbsbox.service.d/old.conf']:
            with self.subTest(name=name),tempfile.TemporaryDirectory() as temporary:
                root=Path(temporary).resolve();layout=release.base.Layout(root)
                saved=root/name;saved.parent.mkdir(parents=True);saved.write_bytes(b'retain existing setup')
                with self.assertRaises(ValueError):release.check_fresh(layout)
                self.assertEqual(saved.read_bytes(),b'retain existing setup')
                self.assertFalse(layout.application.exists());self.assertFalse(layout.data.exists())

    def test_existing_game_state_or_worker_units_refuse_first_install(self):
        for name in ['var/lib/bbsbox-game/state.sqlite',
                     'etc/systemd/system/bbsbox-game-worker@.service',
                     'etc/systemd/system/bbsbox-game-worker.socket.d/override.conf']:
            with self.subTest(name=name),tempfile.TemporaryDirectory() as temporary:
                root=Path(temporary).resolve()
                saved=root/name;saved.parent.mkdir(parents=True);saved.write_bytes(b'keep game data')
                with self.assertRaises(ValueError):release.check_fresh(release.base.Layout(root))
                self.assertEqual(saved.read_bytes(),b'keep game data')


if __name__=='__main__':unittest.main()
