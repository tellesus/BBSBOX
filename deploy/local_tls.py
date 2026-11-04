# SPDX-License-Identifier: AGPL-3.0-or-later
"""Issue a local server leaf from an existing CA without changing that CA or contacting a server."""
import hashlib
import ipaddress
from pathlib import Path
import re
import secrets
import subprocess
import tempfile


def issue(ca, ca_key, names, temporary_parent):
    names=list(dict.fromkeys(names))
    if not 1<=len(names)<=10:raise ValueError('Use one to ten distinct local certificate names')
    entries=[]; verification=[]
    for name in names:
        try:
            address=ipaddress.ip_address(name)
            private=address.is_loopback or address.is_link_local or any(address.version==network.version and address in network for network in map(ipaddress.ip_network,['10.0.0.0/8','172.16.0.0/12','192.168.0.0/16','fc00::/7']))
            if not private:raise ValueError('Certificate address must be local')
            entries.append('IP:'+str(address));verification.append(('-verify_ip',str(address)))
        except ValueError:
            if name!='localhost' and not re.fullmatch(r'bbsbox-(?:game-)?[a-f0-9]{16}\.home\.arpa',name):raise ValueError('Certificate name must be a node-local name or address') from None
            entries.append('DNS:'+name);verification.append(('-verify_hostname',name))
    ca=Path(ca).resolve();ca_key=Path(ca_key).resolve();serial=secrets.token_hex(16)
    with tempfile.TemporaryDirectory(prefix='leaf-',dir=temporary_parent) as temporary:
        work=Path(temporary);work.chmod(0o700)
        (work/'request.cnf').write_text('[req]\ndistinguished_name=dn\nprompt=no\n[dn]\nCN='+names[0]+'\n',encoding='ascii')
        (work/'leaf.ext').write_text('basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName='+','.join(entries)+'\n',encoding='ascii')
        def openssl(*args):return subprocess.run(['openssl',*args],cwd=work,check=True,capture_output=True,timeout=30).stdout
        openssl('req','-config','request.cnf','-new','-newkey','rsa:3072','-nodes','-keyout','server-key.pem','-out','server.csr')
        openssl('x509','-req','-in','server.csr','-CA',str(ca),'-CAkey',str(ca_key),'-set_serial','0x'+serial,'-days','365','-sha256','-extfile','leaf.ext','-out','server.pem')
        for flag,name in verification:openssl('verify','-CAfile',str(ca),flag,name,'server.pem')
        certificate=(work/'server.pem').read_bytes();key=(work/'server-key.pem').read_bytes()
        fingerprint=hashlib.sha256(openssl('x509','-in','server.pem','-outform','DER')).hexdigest()
    return dict(certificate=certificate,key=key,serial=serial,sha256=fingerprint)
