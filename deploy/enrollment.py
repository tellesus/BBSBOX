# SPDX-License-Identifier: AGPL-3.0-or-later
"""Export a public local connection card and CA; never install trust or copy private keys."""
import argparse
import hashlib
import html
import ipaddress
import json
from pathlib import Path
import re
import ssl
import subprocess
from urllib.parse import urlsplit


def read_json(path):
    with Path(path).open('rb') as source: data = source.read(65537)
    if len(data)>65536: raise ValueError('Enrollment input exceeds 64 KiB')
    return json.loads(data)


def ca_fingerprint(ca):
    with Path(ca).open('rb') as source: data = source.read(16385)
    if len(data)>16384 or not re.fullmatch(rb'-----BEGIN CERTIFICATE-----\s+[A-Za-z0-9+/=\s]+-----END CERTIFICATE-----\s*', data):
        raise ValueError('Expected exactly one public PEM certificate')
    der = ssl.PEM_cert_to_DER_cert(data.decode('ascii'))
    fingerprint = ':'.join(re.findall('..', hashlib.sha256(der).hexdigest().upper()))
    # This check uses local certificate/time information only; no online revocation lookup.
    subprocess.run(['openssl', 'verify', '-CAfile', str(ca), str(ca)], check=True, capture_output=True, timeout=10)
    return data, fingerprint


def export(installation, config, ca, output):
    installed = read_json(installation); settings = read_json(config)
    node_id = installed.get('node_id', '')
    if installed.get('format') != 'bbsbox.development-install/v1' or not re.fullmatch(r'node:[a-f0-9]{64}', node_id):
        raise ValueError('Expected development installation metadata')
    hostname = 'bbsbox-'+node_id.split(':')[1][:16]+'.home.arpa'
    if installed.get('hostname') != hostname or settings.get('format') != 'bbsbox.node-config/v1':
        raise ValueError('Node hostname/configuration mismatch')
    name = settings.get('name', '')
    if not isinstance(name, str) or not name.strip() or len(name)>64 or any(ord(char)<32 or 127<=ord(char)<=159 for char in name):
        raise ValueError('Invalid public node name')
    extra = settings.get('additional_origins', [])
    if not isinstance(extra, list) or len(extra)>8: raise ValueError('Too many additional origins')
    origins = [settings['origin'], *extra]
    if len(set(origins)) != len(origins): raise ValueError('Duplicate origins')
    for origin in origins:
        if not isinstance(origin, str): raise ValueError('Origin must be text')
        uri = urlsplit(origin)
        if uri.scheme != 'https' or not uri.hostname or uri.username or uri.password or uri.path or uri.query or uri.fragment or not uri.port or not 1024<=uri.port<=65535:
            raise ValueError('Expected a bare local HTTPS origin')
        if uri.hostname not in [hostname, 'localhost']:
            address = ipaddress.ip_address(uri.hostname)
            allowed = address.is_loopback or address.is_link_local or (address.version == 4 and any(address in ipaddress.ip_network(net) for net in ['10.0.0.0/8','172.16.0.0/12','192.168.0.0/16'])) or (address.version == 6 and address in ipaddress.ip_network('fc00::/7'))
            if not allowed: raise ValueError('Enrollment addresses must be local')
    certificate, fingerprint = ca_fingerprint(ca)
    if installed.get('ca_sha256') != fingerprint: raise ValueError('CA fingerprint differs from the installation record')
    public = dict(format='bbsbox.public-enrollment/v1', name=name, node_id=node_id, hostname=hostname, origins=origins, ca_sha256=fingerprint, ca_file='ca.pem')
    links = ''.join(f'<li><a href="{html.escape(origin, quote=True)}">{html.escape(origin)}</a></li>' for origin in origins)
    card = '<!doctype html>\n<html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>BBSBOX connection card</title><style>body{font:1rem system-ui;max-width:55rem;margin:2rem auto;padding:0 1rem;background:#000;color:#eee}a{color:#4de8e0}code{overflow-wrap:anywhere}li{margin:.6rem 0}@media print{body{background:white;color:black}a{color:black}}</style>'
    card += f'<h1>{html.escape(name)}</h1><p>Join this node’s local Wi-Fi, then open one of these addresses:</p><ul>{links}</ul>'
    card += '<h2>Local certificate</h2><p>Compare this SHA-256 fingerprint with a copy supplied directly by the SysOp before trusting the CA. A copy downloaded from the same connection is not an independent comparison.</p>'
    card += f'<p><code>{fingerprint}</code></p><p><a href="ca.pem" download>Public CA certificate</a></p><p>Use your device’s certificate settings to trust this CA for this node. This card does not change trust settings. Keep your device’s date and time correct. If a certificate warning remains after enrollment, ask the SysOp to check the local address, certificate and clock.</p>'
    card += f'<p>Node ID: <code>{node_id}</code></p></html>\n'
    output = Path(output).absolute()
    if output != output.resolve(): raise ValueError('Output must not traverse symlinks')
    output.mkdir(parents=True, exist_ok=False)
    (output/'ca.pem').write_bytes(certificate)
    (output/'node.json').write_text(json.dumps(public, indent=2)+'\n', encoding='utf-8')
    (output/'connect.html').write_text(card, encoding='utf-8')
    return public


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--installation', type=Path, default=Path('/etc/bbsbox/installation.json'))
    parser.add_argument('--config', type=Path, default=Path('/etc/bbsbox/node.json'))
    parser.add_argument('--ca', type=Path, default=Path('/etc/bbsbox/tls/ca.pem'))
    parser.add_argument('--output', type=Path, required=True); args = parser.parse_args()
    print(json.dumps(export(args.installation, args.config, args.ca, args.output), indent=2))


if __name__ == '__main__': main()
