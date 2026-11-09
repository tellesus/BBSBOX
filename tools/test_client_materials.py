# SPDX-License-Identifier: AGPL-3.0-or-later
"""Client material profile using synthetic dependency/source fixtures, never real license grants."""
import json
from pathlib import Path
import unittest
import client_materials as client
import release_materials as materials
import test_release_materials as fixtures


class ClientMaterials(unittest.TestCase):
    git = fixtures.Materials.git

    def setUp(self):
        fixtures.Materials.setUp(self)
        native = self.repo / client.NATIVE_LOCK
        native.parent.mkdir(parents=True)
        native.write_text((self.repo / 'Cargo.lock').read_text(), encoding='utf-8')
        self.git('add', '.')
        self.git('-c', 'user.name=BBSBOX fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '--quiet', '-m', 'native fixture')
        self.commit = self.git('rev-parse', 'HEAD').decode().strip()

    def test_native_profile_extracts_verified_sources_and_checks_notices(self):
        report = client.build(self.repo, self.commit, self.vendor, self.output)
        self.assertEqual(report['dependencies'], 1)
        self.assertFalse(report['installation_authority'])
        destination = self.root / 'extracted-client'
        client.extract(self.output, self.commit, destination)
        self.assertEqual((destination / 'source' / client.NATIVE_LOCK).read_bytes(), self.git('show', f'{self.commit}:{client.NATIVE_LOCK}'))
        self.assertEqual((destination / 'offline-build.txt').read_bytes(), client.BUILD)
        notice = self.output / 'notices.json'
        value = json.loads(notice.read_text())
        value['target'] = 'debian13-amd64'
        notice.write_bytes(materials.encoded(value))
        with self.assertRaises(ValueError):
            client.verify(self.output, self.commit)

    def test_client_and_node_materials_cannot_be_relabelled(self):
        client.build(self.repo, self.commit, self.vendor, self.output)
        with self.assertRaises(ValueError):
            materials.verify(self.output, self.commit)
        node = self.root / 'node-materials'
        materials.build(self.repo, self.commit, self.vendor, node)
        with self.assertRaises(ValueError):
            client.verify(node, self.commit)

    def test_conflicting_lock_checksums_and_missing_native_source_fail(self):
        conflict = (self.repo / 'Cargo.lock').read_bytes().replace(b'1' * 64, b'2' * 64)
        selected = {'Cargo.lock': (self.repo / 'Cargo.lock').read_bytes()}
        with self.assertRaisesRegex(ValueError, 'Conflicting locked dependency'):
            materials.inspect_dependencies([], selected, extra_locks=(conflict,))
        (self.repo / client.NATIVE_LOCK).unlink()
        self.git('add', '.')
        self.git('-c', 'user.name=BBSBOX fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '--quiet', '-m', 'missing native fixture')
        missing = self.git('rev-parse', 'HEAD').decode().strip()
        with self.assertRaisesRegex(ValueError, 'Missing native client lockfile'):
            client.build(self.repo, missing, self.vendor, self.output)

    def test_client_only_license_supplements_are_bound_to_source_without_affecting_node_materials(self):
        (self.vendor / 'fixture-1.0.0' / 'LICENSE').unlink()
        checksum = self.vendor / 'fixture-1.0.0' / '.cargo-checksum.json'
        contents = json.loads(checksum.read_bytes())
        del contents['files']['LICENSE']
        checksum.write_bytes(materials.encoded(contents))
        name = 'client-fixture-MIT.txt'
        raw = b'Synthetic client license fixture, never an actual grant.\n'
        path = self.repo / client.LICENSES / name
        path.parent.mkdir(parents=True)
        path.write_bytes(raw)
        (self.repo / client.SUPPLEMENTS).write_bytes(materials.encoded({
            'format': 'bbsbox.license-supplements/v1',
            'files': {name: {'sha256': materials.digest(raw), 'source': 'https://example.invalid/fixture'}},
            'packages': {'fixture-1.0.0': {'choice': 'MIT', 'files': [name]}},
        }))
        self.git('add', '.')
        self.git('-c', 'user.name=BBSBOX fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '--quiet', '-m', 'client license fixture')
        commit = self.git('rev-parse', 'HEAD').decode().strip()
        client.build(self.repo, commit, self.vendor, self.output)
        with self.assertRaisesRegex(ValueError, 'reviewed license supplement'):
            materials.build(self.repo, commit, self.vendor, self.root / 'node-missing-license')


if __name__ == '__main__':
    unittest.main()
