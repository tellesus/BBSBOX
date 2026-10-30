# SPDX-License-Identifier: AGPL-3.0-or-later
"""Create a local test CA and TLS leaf using installed OpenSSL; never install trust globally."""
import argparse
import hashlib
import ipaddress
import os
from pathlib import Path
import re
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--state', type=Path, default=Path('.state'))
parser.add_argument('--san', action='append', default=[], help='Additional LAN IP or local DNS name')
args = parser.parse_args()
dest = args.state.resolve() / 'tls'
if dest.exists():
    parser.error(f'{dest} already exists; preserve existing trust material or choose a fresh test-state folder')
names = ['localhost', '127.0.0.1', '::1'] + args.san
entries = []
for name in dict.fromkeys(names):
    try:
        entries.append('IP:' + str(ipaddress.ip_address(name)))
    except ValueError:
        if not re.fullmatch(r'[A-Za-z0-9](?:[A-Za-z0-9.-]{0,251}[A-Za-z0-9])?', name):
            parser.error('Invalid DNS name for --san')
        entries.append('DNS:' + name)
dest.parent.mkdir(parents=True, exist_ok=True)
os.chmod(dest.parent, 0o700)
with tempfile.TemporaryDirectory(prefix='tls-', dir=dest.parent) as temp:
    work = Path(temp)
    (work/'openssl.cnf').write_text('[req]\ndistinguished_name=dn\nprompt=no\n[dn]\nCN=BBSBOX M00\n', encoding='ascii')
    def openssl(*arguments):
        result = subprocess.run(['openssl', *arguments], cwd=work, capture_output=True)
        if result.returncode:
            raise RuntimeError(result.stderr.decode(errors='replace'))
    openssl('req','-config','openssl.cnf','-x509','-newkey','rsa:3072','-sha256','-nodes','-days','365','-subj','/CN=BBSBOX M00 local test CA',
            '-addext','basicConstraints=critical,CA:TRUE','-addext','keyUsage=critical,keyCertSign,cRLSign',
            '-keyout','ca-key.pem','-out','ca.pem')
    openssl('req','-config','openssl.cnf','-new','-newkey','rsa:3072','-nodes','-subj','/CN=localhost','-keyout','server-key.pem','-out','server.csr')
    (work/'leaf.ext').write_text('basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName='+','.join(entries)+'\n', encoding='ascii')
    openssl('x509','-req','-in','server.csr','-CA','ca.pem','-CAkey','ca-key.pem','-CAcreateserial','-days','30','-sha256','-extfile','leaf.ext','-out','server.pem')
    dest.mkdir(mode=0o700)
    for name in ['ca.pem','ca-key.pem','server.pem','server-key.pem']:
        path = dest/name
        path.write_bytes((work/name).read_bytes())
        os.chmod(path, 0o600 if 'key' in name else 0o644)
    # Fingerprint is of the certificate DER, matching normal certificate viewers.
    der = subprocess.check_output(['openssl','x509','-in',str(dest/'ca.pem'),'-outform','DER'])
    print('Local test CA SHA-256:', ':'.join(re.findall('..',hashlib.sha256(der).hexdigest().upper())))
    print('Compare this fingerprint out of band before trusting', dest/'ca.pem')
    print('No system trust store was changed. Leaf valid for 30 days. Keep private keys local.')
