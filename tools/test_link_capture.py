# SPDX-License-Identifier: AGPL-3.0-or-later
import importlib.util
import ipaddress
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('link_capture',Path(__file__).resolve().parents[1]/'deploy/ci/link_capture.py')
capture=importlib.util.module_from_spec(spec);spec.loader.exec_module(capture)


class FrameAuditTests(unittest.TestCase):
    def frame(self,destination):
        ip=ipaddress.ip_address(destination)
        header=bytearray(20 if ip.version==4 else 40);header[0]=0x45 if ip.version==4 else 0x60
        if ip.version==4:header[16:20]=ip.packed
        else:header[24:40]=ip.packed
        return b'\0'*12+(b'\x08\x00' if ip.version==4 else b'\x86\xdd')+header

    def test_local_visitor_and_link_control_addresses_are_distinguished_from_wan(self):
        for address in ['192.168.50.2','fd42:bb50::2','fe80::2','ff02::1']:
            with self.subTest(address=address):self.assertTrue(capture.classify(self.frame(address))[2])
        for address in ['8.8.8.8','192.168.51.2','2001:4860::1','fd42:bb51::2','ff0e::1']:
            with self.subTest(address=address):self.assertFalse(capture.classify(self.frame(address))[2])

    def test_truncated_or_invalid_ip_and_vlan_frames_fail_instead_of_disappearing(self):
        for frame in [b'',b'\0'*12+b'\x08\x00'+b'\x45',b'\0'*12+b'\x86\xdd'+b'\x60',b'\0'*12+b'\x81\x00'+b'\0',b'\0'*12+b'\x08\x00'+b'\x44'+b'\0'*19]:
            with self.assertRaises(ValueError):capture.classify(frame)
        self.assertIsNone(capture.classify(b'\0'*12+b'\x08\x06'+b'\0'*28))
        frame=self.frame('8.8.8.8');tagged=frame[:12]+b'\x81\x00\0\x01'+frame[12:]
        self.assertFalse(capture.classify(tagged)[2])


if __name__=='__main__':unittest.main()
