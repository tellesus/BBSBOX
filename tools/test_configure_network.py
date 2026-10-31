# SPDX-License-Identifier: AGPL-3.0-or-later
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'deploy'))
import configure_network


class InactiveTreeTests(unittest.TestCase):
    def setUp(self):
        self.temporary=tempfile.TemporaryDirectory(); self.root=Path(self.temporary.name).resolve()
        self.config=self.root/'etc/bbsbox'; self.config.mkdir(parents=True)
        self.node=dict(format='bbsbox.development-install/v1',node_id='node:'+'a'*64,hostname='bbsbox-'+'a'*16+'.home.arpa')
        (self.config/'installation.json').write_text(json.dumps(self.node),encoding='utf-8')
        (self.config/'node.json').write_text(json.dumps(dict(format='bbsbox.node-config/v1',bind='127.0.0.1:8443',origin='https://localhost:8443')),encoding='utf-8')
        self.data=self.root/'var/lib/bbsbox/node.sqlite';self.data.parent.mkdir(parents=True);self.data.write_bytes(b'existing community data')
        self.profile=json.loads((ROOT/'deploy/network.example.json').read_text(encoding='utf-8'));self.profile['hostname']='auto'

    def tearDown(self):
        self.assertEqual(self.data.read_bytes(),b'existing community data')
        self.temporary.cleanup()

    def test_interrupted_preparation_is_preserved_and_refused(self):
        marker=self.config/'.network-prepare-incomplete';marker.write_text('inspection required',encoding='utf-8')
        before=(self.config/'node.json').read_bytes()
        with self.assertRaisesRegex(ValueError,'interrupted'):configure_network.preflight(self.root,self.profile)
        self.assertEqual(marker.read_text(encoding='utf-8'),'inspection required')
        self.assertEqual((self.config/'node.json').read_bytes(),before)

    def test_existing_network_and_different_node_name_are_preserved_and_refused(self):
        self.profile['hostname']='bbsbox-'+'b'*16+'.home.arpa'
        with self.assertRaisesRegex(ValueError,'hostname'):configure_network.preflight(self.root,self.profile)
        self.profile['hostname']='auto'
        configured=self.root/'etc/systemd/network/existing.network';configured.parent.mkdir(parents=True);configured.write_text('existing configuration',encoding='utf-8')
        with self.assertRaisesRegex(ValueError,'Existing network'):configure_network.preflight(self.root,self.profile)
        self.assertEqual(configured.read_text(encoding='utf-8'),'existing configuration')

    @unittest.skipUnless(os.name=='posix','Linux inactive-root guard')
    def test_current_root_is_never_a_preparation_target(self):
        with self.assertRaisesRegex(ValueError,'current OS root'):configure_network.preflight(Path('/'),self.profile)


if __name__=='__main__':unittest.main()
