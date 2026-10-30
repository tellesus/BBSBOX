# SPDX-License-Identifier: AGPL-3.0-or-later
"""Record resolved dependency license metadata; dependency source retains its own notices."""
import argparse
import json
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('cargo_metadata', type=Path, help='cargo metadata --locked --filter-platform TARGET --format-version 1 output')
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
cargo = json.loads(args.cargo_metadata.read_text(encoding='utf-8-sig'))
npm = json.loads((root/'web/package-lock.json').read_text(encoding='utf-8'))
entries = [dict(ecosystem='cargo',name=p['name'],version=p['version'],license=p['license'] or 'SEE UPSTREAM NOTICE',source=p.get('source') or 'project') for p in cargo['packages']]
for path, p in npm['packages'].items():
    if not path:
        continue
    installed = root/'web'/path/'package.json'
    metadata = json.loads(installed.read_text(encoding='utf-8')) if installed.is_file() else {}
    entries.append(dict(ecosystem='npm',name=p.get('name') or path.rsplit('node_modules/',1)[-1],version=p['version'],license=p.get('license') or metadata.get('license') or 'SEE UPSTREAM NOTICE',source=p.get('resolved','package-lock.json')))
result = dict(status='Resolved metadata snapshot for M00; includes development/optional packages, not a legal audit or runtime support claim.',cargo_platform='x86_64-pc-windows-gnullvm',dependencies=sorted(entries,key=lambda p:(p['ecosystem'],p['name'],p['version'])))
(root/'dependencies.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8',newline='\n')
print(f'Recorded {len(entries)} dependency entries.')
print('License expressions:', ', '.join(sorted({e['license'] for e in entries})))
