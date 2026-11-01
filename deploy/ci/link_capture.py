# SPDX-License-Identifier: AGPL-3.0-or-later
"""Capture a simulated visitor link during the complete guest boot/restart/shutdown."""
import ipaddress
import json
import socket
import struct
import threading
import time

LOCAL4=ipaddress.ip_network('192.168.50.0/24')
LOCAL6=[ipaddress.ip_network(value) for value in ['fd42:bb50::/64','fe80::/10','ff02::/16']]


def classify(frame):
    if len(frame)<14:raise ValueError('Truncated Ethernet frame')
    kind=frame[12:14];offset=14
    for _ in range(2):
        if kind not in [b'\x81\x00',b'\x88\xa8']:break
        if len(frame)<offset+4:raise ValueError('Truncated VLAN frame')
        kind=frame[offset+2:offset+4];offset+=4
    if kind in [b'\x81\x00',b'\x88\xa8']:raise ValueError('Unsupported nested VLAN frame')
    if kind==b'\x08\x00':
        if len(frame)<offset+20 or frame[offset]>>4!=4 or frame[offset]&15<5 or len(frame)<offset+4*(frame[offset]&15):raise ValueError('Malformed IPv4 frame')
        destination=ipaddress.ip_address(frame[offset+16:offset+20])
        return 4,str(destination),destination in LOCAL4
    if kind==b'\x86\xdd':
        if len(frame)<offset+40 or frame[offset]>>4!=6:raise ValueError('Malformed IPv6 frame')
        destination=ipaddress.ip_address(frame[offset+24:offset+40])
        return 6,str(destination),any(destination in subnet for subnet in LOCAL6)
    return None


class Capture:
    def __init__(self,interface,path):
        self.path=path;self.stopping=threading.Event();self.counts={4:0,6:0};self.non_ip=0;self.violations=[];self.errors=[];self.bytes=0
        self.socket=socket.socket(socket.AF_PACKET,socket.SOCK_RAW,socket.htons(3));self.socket.bind((interface,0));self.socket.settimeout(.1)
        self.file=path.open('xb');self.file.write(struct.pack('<IHHIIII',0xa1b2c3d4,2,4,0,0,65535,1))
        self.thread=threading.Thread(target=self.read,daemon=True);self.thread.start()

    def __enter__(self):return self

    def __exit__(self,*args):self.report=self.finish()

    def read(self):
        try:
            while True:
                try:frame,address=self.socket.recvfrom(65535)
                except socket.timeout:
                    if self.stopping.is_set():break
                    continue
                except OSError as error:
                    if error.errno in [19,100]:break  # The disposable peer/link disappeared during shutdown.
                    raise
                if address[2]==socket.PACKET_OUTGOING:continue  # Observe only guest-to-visitor traffic.
                self.bytes+=len(frame)
                if self.bytes>16*1024*1024:raise ValueError('Capture exceeded its 16 MiB evidence budget')
                now=time.time_ns();self.file.write(struct.pack('<IIII',now//1_000_000_000,(now%1_000_000_000)//1000,len(frame),len(frame)));self.file.write(frame)
                item=classify(frame)
                if item is None:self.non_ip+=1;continue
                family,destination,local=item;self.counts[family]+=1
                if not local and len(self.violations)<32:self.violations.append(dict(family=family,destination=destination))
        except (OSError,ValueError) as error:self.errors.append(str(error))

    def finish(self):
        self.stopping.set();self.thread.join(timeout=2);self.socket.close()
        if self.thread.is_alive():self.errors.append('Capture reader did not stop')
        self.file.close()
        report=dict(status='passed' if all(self.counts.values()) and not self.violations and not self.errors else 'failed',guest_ipv4_frames=self.counts[4],guest_ipv6_frames=self.counts[6],non_ip_frames=self.non_ip,nonlocal_destinations=self.violations,errors=self.errors,scope='Guest-to-visitor frames from simulated link appearance through guest shutdown; local IPv6 link-control multicast is allowed')
        self.path.with_suffix('.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
        return report
