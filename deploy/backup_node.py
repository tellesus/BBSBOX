# SPDX-License-Identifier: AGPL-3.0-or-later
"""Cold backup of a signed installation; restore remains isolated and recovery-blocked."""
import argparse
import json
from pathlib import Path
import backup_development


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--recipient',required=True)
    args=parser.parse_args()
    print(json.dumps(backup_development.backup(args.output,args.recipient,release=True),indent=2))


if __name__=='__main__':main()
