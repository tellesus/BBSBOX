# SPDX-License-Identifier: AGPL-3.0-or-later
"""Restore a lost or retired foundation node onto fresh signed Debian media."""
import argparse
import json
from pathlib import Path

import install_release
from restore_provisioning import Request


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ['bundle','verifier','archive','identity-file']:parser.add_argument('--'+name,type=Path,required=True)
    parser.add_argument('--public-key',required=True)
    parser.add_argument('--minimum-sequence',type=int,required=True)
    parser.add_argument('--node',required=True)
    parser.add_argument('--source',choices=['lost','retired'],required=True)
    parser.add_argument('--reason',required=True)
    parser.add_argument('--enable',action='store_true')
    args=parser.parse_args()
    request=Request(args.archive.absolute(),args.identity_file.absolute(),args.node,args.source,args.reason,args.minimum_sequence)
    print(json.dumps(install_release.install(args.bundle,args.verifier,args.public_key,None,args.enable,recovery=request),indent=2))


if __name__=='__main__':main()
