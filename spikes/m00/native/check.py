# SPDX-License-Identifier: AGPL-3.0-or-later
"""Run the real bundled WebView and verify its native-command/navigation result."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
from urllib.parse import urlsplit

ROOT=Path(__file__).resolve().parents[3]


def check(binary):
    with tempfile.TemporaryDirectory(prefix='native-client-',dir=ROOT/'work') as temporary:
        output=Path(temporary)/'report.json'
        flags=subprocess.CREATE_NO_WINDOW if os.name=='nt' else 0
        result=subprocess.run([str(binary.resolve(strict=True)),'--check',str(output)],
                              capture_output=True,timeout=45,creationflags=flags)
        if result.returncode:
            raise RuntimeError(result.stderr[-4096:].decode(errors='replace'))
        report=json.loads(output.read_text(encoding='utf-8'))
        assert report['format']=='bbsbox.native-client-probe/v1'
        assert re.fullmatch(r'person:[0-9a-f]{64}',report['person'])
        assert report['native_identity_command'] is True
        assert report['remote_navigation_denied'] is True
        url=urlsplit(report['bundled_url'])
        assert (url.scheme,url.hostname) in [('tauri','localhost'),('http','tauri.localhost')]
        assert report['signed_distribution']=='pending' and report['key_persistence']=='pending'
        print(json.dumps(report,indent=2))


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,required=True)
    arguments=parser.parse_args()
    check(arguments.binary)
