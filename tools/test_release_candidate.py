# SPDX-License-Identifier: AGPL-3.0-or-later
import sys
import tempfile
import unittest
from pathlib import Path
import release_candidate as candidate


class CandidateCommands(unittest.TestCase):
    def test_bounded_success_failure_and_timeout(self):
        self.assertEqual(candidate.command([sys.executable,'-c','print("{\\"ok\\":true}")']),{'ok':True})
        with self.assertRaises(ValueError): candidate.bounded_run([sys.executable,'-c','raise SystemExit(7)'])
        with self.assertRaises(ValueError): candidate.bounded_run([sys.executable,'-c','import time;time.sleep(10)'],timeout=0.1)

    def test_both_output_pipes_are_bounded_while_the_child_is_running(self):
        for stream in ('stdout','stderr'):
            with self.assertRaises(ValueError): candidate.bounded_run([sys.executable,'-c',f'import sys,time;sys.{stream}.write("x"*100000);sys.{stream}.flush();time.sleep(10)'])

    def test_wrong_key_role_and_provenance_fail_before_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();key=root/'key.json';key.write_text('{"format":"bbsbox.backup-recipient/v1"}')
            with self.assertRaises(ValueError): candidate.public_info(key)
            output=root/'candidate'
            with self.assertRaises(ValueError): candidate.prepare(root/'tool',root/'binary',root/'materials','bad',root/'evidence',root/'private',key,0,0,output)
            self.assertFalse(output.exists())


if __name__=='__main__': unittest.main()
