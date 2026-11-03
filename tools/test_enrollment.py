# SPDX-License-Identifier: AGPL-3.0-or-later
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT/'deploy'))
import enrollment


class EnrollmentTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory(); cls.state = Path(cls.temporary.name).resolve()
        subprocess.run([sys.executable, str(ROOT/'spikes/m00/tools/init_tls.py'), '--state', str(cls.state)], check=True, capture_output=True)
        cls.ca = cls.state/'tls/ca.pem'
        cls.fingerprint = enrollment.ca_fingerprint(cls.ca)[1]

    @classmethod
    def tearDownClass(cls): cls.temporary.cleanup()

    def setUp(self):
        self.work = tempfile.TemporaryDirectory(); self.root = Path(self.work.name).resolve()
        self.installation = dict(format='bbsbox.development-install/v1', node_id='node:'+'a'*64, hostname='bbsbox-'+'a'*16+'.home.arpa', ca_sha256=self.fingerprint)
        self.config = dict(format='bbsbox.node-config/v1', name='<Neighborhood & node>', origin='https://'+self.installation['hostname']+':8443', additional_origins=['https://192.168.50.1:8443','https://[fd42:bb50::1]:8443'], database='private path', certificate_key='private key path')

    def tearDown(self): self.work.cleanup()

    def export(self):
        (self.root/'installation.json').write_text(json.dumps(self.installation), encoding='utf-8')
        (self.root/'config.json').write_text(json.dumps(self.config), encoding='utf-8')
        return enrollment.export(self.root/'installation.json', self.root/'config.json', self.ca, self.root/'public')

    def test_public_export_matches_ca_and_omits_private_paths_and_keys(self):
        public = self.export(); output = self.root/'public'
        self.assertEqual({path.name for path in output.iterdir()}, {'ca.pem', 'node.json', 'connect.html'})
        self.assertEqual((output/'ca.pem').read_bytes(), self.ca.read_bytes())
        self.assertEqual(public['ca_sha256'], self.fingerprint)
        card = (output/'connect.html').read_text(encoding='utf-8')
        self.assertIn('&lt;Neighborhood &amp; node&gt;', card)
        self.assertIn(self.fingerprint, card)
        self.assertNotIn('private', (output/'node.json').read_text(encoding='utf-8'))
        with self.assertRaises(FileExistsError): self.export()

    def test_signed_installation_uses_the_same_public_enrollment_boundary(self):
        self.installation['format']='bbsbox.install/v1'
        self.installation['release_id']='b'*64
        self.assertEqual(self.export()['node_id'],self.installation['node_id'])
        self.assertNotIn('release_id',(self.root/'public/node.json').read_text())

    def test_different_ca_or_hostname_is_rejected_before_export(self):
        self.installation['ca_sha256'] = '00:'*31+'00'
        with self.assertRaises(ValueError): self.export()
        self.assertFalse((self.root/'public').exists())
        self.installation['ca_sha256'] = self.fingerprint; self.installation['hostname'] = 'other.home.arpa'
        with self.assertRaises(ValueError): self.export()

    def test_public_or_injected_origin_and_certificate_plus_private_key_are_rejected(self):
        for origin in ['https://example.com:8443','https://8.8.8.8:8443','https://localhost:8443/path','https://localhost:8443/','https://localhost:8443#fragment','https://user@localhost:8443']:
            self.config['additional_origins'] = [origin]
            with self.subTest(origin=origin), self.assertRaises(ValueError): self.export()
        combined = self.root/'combined.pem'
        combined.write_bytes(self.ca.read_bytes()+(self.state/'tls/ca-key.pem').read_bytes())
        with self.assertRaises(ValueError): enrollment.ca_fingerprint(combined)


if __name__ == '__main__': unittest.main()
