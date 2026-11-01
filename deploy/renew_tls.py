# SPDX-License-Identifier: AGPL-3.0-or-later
"""Prepare a fresh local TLS leaf and atomically select its complete pair; never replace the node CA."""
import argparse
import json
import os
from pathlib import Path
import tempfile
from urllib.parse import urlsplit

import enrollment
import local_tls
from configure_network import target, write_new


def renew(root):
    if os.name!='posix' or os.geteuid()!=0:raise ValueError('Use the Debian development node as root')
    root=Path(root).absolute()
    if root!=root.resolve():raise ValueError('Root must not traverse symlinks')
    import fcntl
    lock_path=target(root,'etc/bbsbox/.tls-renewal.lock')
    lock=os.open(lock_path,os.O_RDWR|os.O_CREAT|os.O_NOFOLLOW,0o600)
    try:
        fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        config_path=target(root,'etc/bbsbox/node.json')
        if config_path.is_symlink() or config_path.stat().st_uid!=0:raise ValueError('Expected root-owned local node configuration')
        with config_path.open('rb') as stream:before=stream.read(65537)
        if len(before)>65536:raise ValueError('Configuration exceeds 64 KiB')
        config=json.loads(before);installed=enrollment.read_json(root/'etc/bbsbox/installation.json')
        with tempfile.TemporaryDirectory(prefix='renew-check-',dir=root/'etc/bbsbox') as temporary:
            public=enrollment.export(root/'etc/bbsbox/installation.json',config_path,root/'etc/bbsbox/tls/ca.pem',Path(temporary)/'public')
        names=[public['hostname'],*[urlsplit(origin).hostname for origin in public['origins']]]
        prepared=local_tls.issue(root/'etc/bbsbox/tls/ca.pem',root/'etc/bbsbox/tls/ca-key.pem',names,root/'etc/bbsbox')
        gid=config_path.stat().st_gid
        relative='etc/bbsbox/certificates/leaf-'+prepared['serial']
        directory=target(root,relative);directory.mkdir(parents=True,exist_ok=False,mode=0o750);os.chown(directory,0,gid);directory.chmod(0o750)
        directory.parent.chmod(0o755)
        write_new(root,relative+'/server.pem',prepared['certificate'],0o644,gid)
        write_new(root,relative+'/server-key.pem',prepared['key'],0o640,gid)
        for durable in [directory,directory.parent,config_path.parent]:
            descriptor=os.open(durable,os.O_RDONLY|os.O_DIRECTORY)
            try:os.fsync(descriptor)
            finally:os.close(descriptor)
        config.update(certificate='/'+relative+'/server.pem',certificate_key='/'+relative+'/server-key.pem')
        staged=write_new(root,'etc/bbsbox/node.renew-'+prepared['serial']+'.json',(json.dumps(config,indent=2)+'\n').encode(),0o640,gid)
        if config_path.read_bytes()!=before:raise ValueError('Configuration changed during preparation; staged leaf was not activated')
        staged.replace(config_path)
        directory_fd=os.open(config_path.parent,os.O_RDONLY|os.O_DIRECTORY)
        try:os.fsync(directory_fd)
        finally:os.close(directory_fd)
        return dict(format='bbsbox.development-tls-renewal/v1',node_id=installed['node_id'],ca_sha256=public['ca_sha256'],leaf_sha256=prepared['sha256'],serial=prepared['serial'],restart_required=True,ca_replaced=False)
    finally:os.close(lock)


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--root',type=Path,default=Path('/'));parser.add_argument('--development',action='store_true',required=True);args=parser.parse_args()
    print(json.dumps(renew(args.root),indent=2))


if __name__=='__main__':main()
