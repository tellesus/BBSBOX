# SPDX-License-Identifier: AGPL-3.0-or-later
"""The syscall auditor must reject attempted egress and cannot pass empty evidence."""
import unittest
from offline_smoke import audit_trace

LISTENER = '100 bind(8, {sa_family=AF_INET}, 16) = 0\n100 listen(8, 1024) = 0\n100 accept4(8, {sa_family=AF_INET}, [16], SOCK_CLOEXEC) = 9\n'


class TraceAuditTests(unittest.TestCase):
    def test_actual_tcp_serving_without_outbound_destination_is_allowed(self):
        self.assertEqual(audit_trace(LISTENER+'100 sendto(9, "test", 4, MSG_NOSIGNAL, NULL, 0) = 4\n')['outbound_connects'], 0)

    def test_failed_ipv4_and_ipv6_attempts_are_rejected(self):
        for family in ('AF_INET', 'AF_INET6'):
            with self.subTest(family=family), self.assertRaises(ValueError):
                audit_trace(LISTENER+f'100 connect(9, {{sa_family={family}}}, 16) = -1 ENETUNREACH\n')

    def test_datagram_destination_is_rejected(self):
        with self.assertRaises(ValueError):
            audit_trace(LISTENER+'100 sendto(9, "dns", 3, 0, {sa_family=AF_INET}, 16) = 3\n')

    def test_empty_or_partial_evidence_does_not_pass(self):
        for text in ('', 'bind(8, ...) = 0\nlisten(8, 1) = 0\n'):
            with self.subTest(text=text), self.assertRaises(ValueError):
                audit_trace(text)

    def test_unfinished_outbound_attempt_does_not_pass(self):
        with self.assertRaises(ValueError):
            audit_trace(LISTENER+'100 connect(9, {sa_family=AF_INET}, 16 <unfinished ...>\n')

    def test_explicit_message_destination_is_rejected(self):
        with self.assertRaises(ValueError):
            audit_trace(LISTENER+'100 sendmsg(9, {msg_name={sa_family=AF_INET}}, 0) = 1\n')


if __name__ == '__main__':
    unittest.main()
