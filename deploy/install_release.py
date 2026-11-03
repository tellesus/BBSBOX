# SPDX-License-Identifier: AGPL-3.0-or-later
"""Offline signed first install on a fresh Debian appliance. Does not upgrade an existing node."""
import argparse
import json
import os
from pathlib import Path
import re
import stat
import sys
import tempfile
import unicodedata

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'tools'))
import release_candidate as candidate
import install_development as base
import backup_development as backup
import enrollment


def protected_file(path):
    metadata=path.lstat()
    if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid!=0 or metadata.st_mode&0o022:
        raise ValueError('Use a root-owned protected bootstrap verifier')


def check_fresh(layout):
    base.check_fresh(layout)
    if layout.application.exists() and any(layout.application.iterdir()):
        raise ValueError('Signed first install refuses an existing application tree')
    control=layout.root/'var/lib/bbsbox-maintenance/release-control'
    if control.exists() or control.is_symlink():
        raise ValueError('Signed first install never resets existing release control')


def control(tool,directory,action,*options):
    return candidate.command([tool,'release-control-'+action,'--directory',directory,*options])['state']


def install(bundle,verifier,public_key,name,enable=False):
    base.platform_preflight()
    import fcntl
    layout=base.Layout(Path('/'))
    check_fresh(layout)
    verifier=Path(verifier).absolute()
    if verifier!=verifier.resolve():raise ValueError('Bootstrap verifier must not traverse links')
    protected_file(verifier)
    if not name.strip() or len(name)>64 or any(unicodedata.category(c)=='Cc' for c in name):raise ValueError('Invalid public node name')
    descriptor=os.open('/run/bbsbox-maintenance.lock',os.O_RDWR|os.O_CREAT|os.O_NOFOLLOW,0o600)
    try:
        metadata=os.fstat(descriptor)
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid!=0 or metadata.st_mode&0o077:raise ValueError('Unexpected maintenance lock ownership/mode')
        fcntl.flock(descriptor,fcntl.LOCK_EX|fcntl.LOCK_NB)
        check_fresh(layout)
        previous_umask=os.umask(0o077)
        try:
            maintenance=Path('/var/lib/bbsbox-maintenance');backup.prepare_stage(maintenance)
            layout.application.mkdir(parents=True,exist_ok=True,mode=0o755);layout.application.chmod(0o755)
            releases=layout.application/'releases';releases.mkdir(mode=0o755);releases.chmod(0o755)
            with tempfile.TemporaryDirectory(prefix='.install-',dir=releases) as temporary:
                staged=Path(temporary)/'verified'
                verified=candidate.inspect(verifier,Path(bundle),public_key,staged)
                release_id=verified['release_id'];release=releases/release_id
                if release.exists() or release.is_symlink():raise ValueError('Release output already exists')
                staged.rename(release)
            for path in release.rglob('*'):
                path.chmod(0o755 if path.is_dir() or path.name=='bbsbox-node' else 0o644)
            release.chmod(0o755)
            operator_directory=layout.application/'maintenance';operator_directory.mkdir(mode=0o700)
            operator=operator_directory/'bbsbox-operator'
            candidate.copy_file(release/'bbsbox-node',operator);operator.chmod(0o700)
            root_control=maintenance/'release-control'
            initial=control(operator,root_control,'init','--public-key',public_key)
            request=os.urandom(16).hex()
            pending=control(operator,root_control,'begin','--revision',str(initial['revision']),'--bundle',release,'--request',request)
            # Persist the bootstrap ticket before creating node identity/configuration.
            ticket=maintenance/'first-install.json'
            base.write_new(ticket,base.payload.encoded(dict(format='bbsbox.first-install/v1',release_id=release_id,request=request,revision=pending['revision'])),0o600,0,0)
            uid,gid=base.service_account()
            layout.config.mkdir(mode=0o750,parents=True,exist_ok=True);os.chown(layout.config,0,gid);layout.config.chmod(0o750)
            layout.data.mkdir(mode=0o700,parents=True,exist_ok=True);os.chown(layout.data,uid,gid);layout.data.chmod(0o700)
            config=dict(format='bbsbox.node-config/v1',name=name,bind='127.0.0.1:8443',origin='https://localhost:8443',database=str(layout.data/'node.sqlite'),certificate=str(layout.config/'tls/server.pem'),certificate_key=str(layout.config/'tls/server-key.pem'))
            config_path=layout.config/'node.json';base.write_new(config_path,base.payload.encoded(config),0o640,0,gid)
            initialized=json.loads(base.run(['runuser','-u','bbsbox','--',str(release/'bbsbox-node'),'--config',str(config_path),'--initialize']))
            node_id=initialized.get('node_id','')
            if initialized.get('format')!='bbsbox.node-init/v1' or not re.fullmatch(r'node:[a-f0-9]{64}',node_id):raise ValueError('Unexpected node initialization response')
            hostname,fingerprint=base.tls_material(layout.config/'tls',node_id,gid)
            for needed in [config_path,layout.config/'tls/server.pem',layout.config/'tls/server-key.pem']:
                base.run(['runuser','-u','bbsbox','--','/usr/bin/test','-r',str(needed)])
            base.run(['runuser','-u','bbsbox','--','/usr/bin/test','!','-r',str(layout.config/'tls/ca-key.pem')])
            installed=dict(format='bbsbox.install/v1',node_id=node_id,hostname=hostname,origin=config['origin'],ca_sha256=fingerprint,release_id=release_id,release_sequence=verified['sequence'],source_commit=verified['source_commit'],unit_enabled=enable)
            base.write_new(layout.config/'installation.json',base.payload.encoded(installed),0o640,0,gid)
            layout.unit.parent.mkdir(parents=True,exist_ok=True)
            base.write_new(layout.unit,(release/'bbsbox.service').read_bytes(),0o644,0,0)
            (layout.application/'current').symlink_to(release,target_is_directory=True)
            base.run(['systemd-analyze','verify','--man=no',str(layout.unit)])
            public=layout.application/'enrollment';enrollment.export(layout.config/'installation.json',config_path,layout.config/'tls/ca.pem',public)
            public.chmod(0o755)
            for path in public.iterdir():path.chmod(0o644)
            # Synchronize file content/ownership/modes and deepest directory entries first.
            directories={release,operator_directory,layout.application,layout.config,layout.data,layout.unit.parent}
            for tree in [release,layout.config,layout.data]:
                for path in tree.rglob('*'):
                    if path.is_dir():directories.add(path)
                    else:
                        entry=os.open(path,os.O_RDONLY|os.O_NOFOLLOW)
                        try:os.fsync(entry)
                        finally:os.close(entry)
            for directory in sorted(directories,key=lambda p:len(p.parts),reverse=True):
                entry=os.open(directory,os.O_RDONLY|os.O_DIRECTORY)
                try:os.fsync(entry)
                finally:os.close(entry)
            control(operator,root_control,'complete','--revision',str(pending['revision']),'--request',request,'--release-id',release_id)
            candidate.command([operator,'release-control-boot'])
            if enable:
                wanted=layout.unit.parent/'multi-user.target.wants';wanted.mkdir(mode=0o755,exist_ok=True);wanted.chmod(0o755)
                (wanted/layout.unit.name).symlink_to(layout.unit)
                entry=os.open(wanted,os.O_RDONLY|os.O_DIRECTORY)
                try:os.fsync(entry)
                finally:os.close(entry)
            return installed
        finally:os.umask(previous_umask)
    finally:os.close(descriptor)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bundle',type=Path,required=True)
    parser.add_argument('--verifier',type=Path,required=True)
    parser.add_argument('--public-key',required=True)
    parser.add_argument('--name',default='BBSBOX')
    parser.add_argument('--enable',action='store_true')
    args=parser.parse_args()
    print(json.dumps(install(args.bundle,args.verifier,args.public_key,args.name,args.enable),indent=2))


if __name__=='__main__':main()
