# SPDX-License-Identifier: AGPL-3.0-or-later
import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]/'deploy'))
import network_profile as network


class ProfileTests(unittest.TestCase):
    def setUp(self):
        self.profile = json.loads((Path(__file__).resolve().parents[1]/'deploy/network.example.json').read_text(encoding='utf-8'))

    def test_rejects_public_default_overlapping_and_injected_network_configuration(self):
        for addresses in [
            ['0.0.0.0/0'], ['8.8.8.8/24'], ['198.51.100.1/24'], ['2001:db8::1/64'],
            ['127.0.0.1/8'], ['fe80::1/64'], ['192.168.50.0/24'], ['192.168.50.255/24'],
            ['192.168.50.1/32'], ['192.168.50.1/24', '192.168.51.1/24'],
        ]:
            profile = copy.deepcopy(self.profile); profile['interfaces'][0]['addresses'] = addresses
            with self.subTest(addresses=addresses), self.assertRaises(ValueError): network.render(profile)
        for name in ['lo', '*', 'eth0" accept', 'eth0\n', '../eth0', 'x'*16]:
            profile = copy.deepcopy(self.profile); profile['interfaces'][0]['name'] = name
            with self.subTest(name=name), self.assertRaises(ValueError): network.render(profile)
        for subnet in ['192.168.50.2/24', 'fd42:bb50::2/64']:
            profile = copy.deepcopy(self.profile)
            profile['interfaces'].append(dict(name='peer0', role='peer', addresses=[subnet]))
            with self.assertRaises(ValueError): network.render(profile)

    def test_unknown_roles_fields_and_nonlocal_names_fail_before_output_creation(self):
        for key, value in [('hostname', 'example.com'), ('https_port', True), ('https_port', 443), ('interfaces', [])]:
            profile = copy.deepcopy(self.profile); profile[key] = value
            with self.subTest(key=key), self.assertRaises(ValueError): network.render(profile)
        profile = copy.deepcopy(self.profile); profile['interfaces'][0]['role'] = 'wan'
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)/'invalid'
            with self.assertRaises(ValueError): network.prepare(profile, output)
            self.assertFalse(output.exists())
        self.profile['upstream_dns'] = '8.8.8.8'
        with self.assertRaises(ValueError): network.render(self.profile)

    def test_preparation_keeps_existing_files_and_does_not_activate_anything(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)/'prepared'
            names = network.prepare(self.profile, output)
            saved = {name:(output/name).read_bytes() for name in names}
            with self.assertRaises(FileExistsError): network.prepare(self.profile, output)
            self.assertEqual(saved, {name:(output/name).read_bytes() for name in names})
            origins = json.loads(saved['origins.json'])
            self.assertEqual(origins['additional_origins'], ['https://192.168.50.1:8443', 'https://[fd42:bb50::1]:8443'])


if __name__ == '__main__': unittest.main()
