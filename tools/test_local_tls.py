# SPDX-License-Identifier: AGPL-3.0-or-later
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'deploy'))
import local_tls


class LocalLeafTests(unittest.TestCase):
    def test_new_leaf_keeps_ca_and_verifies_every_local_name_without_serial_side_effects(self):
        with tempfile.TemporaryDirectory() as directory:
            state=Path(directory).resolve()
            subprocess.run([sys.executable,str(ROOT/'spikes/m00/tools/init_tls.py'),'--state',str(state)],check=True,capture_output=True)
            ca=state/'tls/ca.pem';key=state/'tls/ca-key.pem'; original=(ca.read_bytes(),key.read_bytes())
            names=['bbsbox-'+'a'*16+'.home.arpa','192.168.50.1','fd42:bb50::1']
            first=local_tls.issue(ca,key,names,state); second=local_tls.issue(ca,key,names,state)
            self.assertNotEqual(first['serial'],second['serial']);self.assertNotEqual(first['key'],second['key'])
            self.assertEqual(original,(ca.read_bytes(),key.read_bytes()));self.assertFalse((state/'tls/ca.srl').exists())
            issued=state/'issued.pem';issued.write_bytes(first['certificate'])
            subprocess.run(['openssl','verify','-CAfile',str(ca),'-verify_hostname',names[0],str(issued)],check=True,capture_output=True)
            wrong=subprocess.run(['openssl','verify','-CAfile',str(ca),'-verify_ip','192.168.50.2',str(issued)],capture_output=True)
            self.assertNotEqual(wrong.returncode,0)

    def test_public_or_injected_names_fail_before_key_generation(self):
        with tempfile.TemporaryDirectory() as directory:
            for name in ['example.com','8.8.8.8','2001:4860::1','localhost\nsubjectAltName=DNS:example.com']:
                with self.subTest(name=name),self.assertRaises(ValueError):local_tls.issue('absent','absent',[name],directory)
            self.assertEqual(list(Path(directory).iterdir()),[])


if __name__=='__main__':unittest.main()
