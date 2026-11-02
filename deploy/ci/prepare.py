# SPDX-License-Identifier: AGPL-3.0-or-later
"""Prepare a minimal developer image context or safely extract its exported OS tree."""
import argparse
from pathlib import Path, PurePosixPath
import subprocess
import sys
import tarfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT/'tools'))
import runtime_bundle as bundle


def destination(path):
    path = Path(path).absolute()
    work = (ROOT/'work').resolve()
    if path != path.resolve() or not path.is_relative_to(work) or path == work:
        raise ValueError('CI output must be a fresh directory inside repository work/')
    path.mkdir(parents=True, exist_ok=False)
    return path


def context(payload, output):
    bundle.verify(payload)
    output = destination(output)
    files = {'Dockerfile': ROOT/'deploy/ci/Dockerfile', 'deploy/install_development.py': ROOT/'deploy/install_development.py', 'deploy/enrollment.py': ROOT/'deploy/enrollment.py', 'deploy/diagnose.py': ROOT/'deploy/diagnose.py', 'tools/runtime_bundle.py': ROOT/'tools/runtime_bundle.py'}
    files.update({'deploy/'+name: ROOT/'deploy'/name for name in ['configure_network.py','network_profile.py','local_tls.py','renew_tls.py','backup_development.py']})
    files.update({'payload/'+name: Path(payload)/name for name in bundle.MEMBERS | {'manifest.json'}})
    for name, source in files.items():
        target = output/name
        target.parent.mkdir(parents=True, exist_ok=True)
        bundle.copy_bounded(source, target)


def validate_archive(members):
    names = set()
    symlinks = set()
    for member in members:
        path = PurePosixPath(member.name)
        if path.is_absolute() or '..' in path.parts or not (member.isfile() or member.isdir() or member.issym() or member.islnk()):
            raise ValueError('Unsupported/escaping OS archive member')
        name = str(path)
        if name in names:
            raise ValueError('Duplicate OS archive member')
        names.add(name)
        if member.issym(): symlinks.add(name)
    for member in members:
        if any(str(parent) in symlinks for parent in PurePosixPath(member.name).parents):
            raise ValueError('OS archive would write through a symlink')
        if member.islnk():
            target = PurePosixPath(member.linkname)
            if target.is_absolute() or '..' in target.parts or str(target) not in names or str(target) in symlinks or any(str(parent) in symlinks for parent in target.parents):
                raise ValueError('Escaping OS archive hardlink')


def extract(archive, output):
    if sys.platform!='linux': raise ValueError('OS extraction is a Linux CI operation')
    with tarfile.open(archive) as source: validate_archive(source.getmembers())
    output = destination(output)
    subprocess.run(['tar', '--numeric-owner', '--same-owner', '--same-permissions', '-xf', str(Path(archive).resolve()), '-C', str(output)], check=True)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    sub=parser.add_subparsers(dest='command',required=True)
    prepare=sub.add_parser('context');prepare.add_argument('--payload',type=Path,required=True);prepare.add_argument('--output',type=Path,required=True)
    unpack=sub.add_parser('extract');unpack.add_argument('--archive',type=Path,required=True);unpack.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    if args.command=='context':context(args.payload,args.output)
    else:extract(args.archive,args.output)


if __name__=='__main__':main()
