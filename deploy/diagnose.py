# SPDX-License-Identifier: AGPL-3.0-or-later
"""Read local node, service, network, certificate and clock state without contacting any server."""
import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import subprocess

import enrollment


def command(*arguments):
    try:
        result=subprocess.run(arguments,capture_output=True,timeout=10)
        return dict(ok=result.returncode==0,output=result.stdout[:65536].decode(errors='replace'),error=result.stderr[:2048].decode(errors='replace'))
    except (OSError,subprocess.TimeoutExpired) as error:
        return dict(ok=False,output='',error=str(error))


def inspect():
    if os.name!='posix':raise ValueError('Run this local diagnostic on the Debian node')
    installed=enrollment.read_json('/etc/bbsbox/installation.json')
    config=enrollment.read_json('/etc/bbsbox/node.json')
    report=dict(format='bbsbox.local-diagnostic/v1',time_utc=datetime.now(timezone.utc).isoformat(),node_id=installed.get('node_id'),name=config.get('name'),hostname=installed.get('hostname'),origins=[config.get('origin'),*config.get('additional_origins',[])],checks={})
    checks=report['checks']
    checks['runtime_service']=command('systemctl','show','bbsbox.service','--property=ActiveState,SubState,Result,User,MainPID')
    checks['runtime_service']['ok'] &= 'ActiveState=active' in checks['runtime_service']['output'] and 'User=bbsbox\n' in checks['runtime_service']['output']
    checks['preparation_finished']=dict(ok=not Path('/etc/bbsbox/.network-prepare-incomplete').exists())
    try:
        _,fingerprint=enrollment.ca_fingerprint('/etc/bbsbox/tls/ca.pem')
        checks['local_ca']=dict(ok=fingerprint==installed.get('ca_sha256'),sha256=fingerprint)
    except (OSError,ValueError,subprocess.SubprocessError) as error:checks['local_ca']=dict(ok=False,error=str(error))
    checks['leaf_chain']=command('openssl','verify','-CAfile','/etc/bbsbox/tls/ca.pem',config['certificate'])
    checks['leaf_valid_for_next_week']=command('openssl','x509','-in',config['certificate'],'-noout','-checkend','604800')
    report['certificate']=command('openssl','x509','-in',config['certificate'],'-noout','-dates','-ext','subjectAltName')
    checks['packet_boundary']=command('nft','-j','list','table','inet','bbsbox_boundary')
    if checks['packet_boundary']['ok']:
        try:
            data=json.loads(checks['packet_boundary'].pop('output'))
            chains={item['chain']['name']:item['chain'].get('policy') for item in data['nftables'] if 'chain' in item}
            checks['packet_boundary'].update(ok=all(chains.get(name)=='drop' for name in ['input','output','forward']),chain_policies=chains)
        except (ValueError,KeyError,TypeError):checks['packet_boundary']['ok']=False
    for family in ['-4','-6']:
        check=command('ip',family,'-j','route','show','table','all')
        if check['ok']:
            try:
                routes=json.loads(check.pop('output'));check.update(ok=not any(route.get('gateway') or route.get('dst')=='default' for route in routes),routes=routes)
            except (ValueError,TypeError):check['ok']=False
        checks['routes'+family]=check
    checks['local_dns_service']=command('systemctl','is-active','bbsbox-dns.service')
    for unit in ['systemd-timesyncd.service','systemd-resolved.service','apt-daily.service','apt-daily-upgrade.service','nftables.service','systemd-network-generator.service']:
        check=command('systemctl','is-enabled',unit)
        check['ok']=check['output'].strip()=='masked';checks[unit]=check
    report['status']='passed' if all(check['ok'] for check in checks.values()) else 'attention'
    report['scope']='Local configuration/service diagnostics; full firewall semantics, browser reachability and device enrollment have separate acceptance checks.'
    return report


def main():
    argparse.ArgumentParser(description=__doc__).parse_args()
    report=inspect(); print(json.dumps(report,indent=2));raise SystemExit(0 if report['status']=='passed' else 1)


if __name__=='__main__':main()
