# SPDX-License-Identifier: AGPL-3.0-or-later
"""Validate roadmap references and reading packets without application dependencies."""
from pathlib import Path
import json
import re
import sys
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
DEV = ROOT / 'docs/development'
errors = []


def require(condition, message):
    if not condition:
        errors.append(message)


def read(path):
    return path.read_text(encoding='utf-8')


def ids(text, prefix):
    return set(re.findall(r'\b' + prefix + r'\d{2}\b', text))


def rows(text):
    return [line.split('|')[1:-1] for line in text.splitlines() if line.startswith('| ')]


def validate():
    graph = json.loads(read(DEV / 'roadmap.json'))
    modules = {m['id']: m for m in graph['modules']}
    require(len(modules) == len(graph['modules']), 'Duplicate module IDs')
    expected = {f'M{i:02}' for i in range(20)} | {f'O{i:02}' for i in range(1, 13)}
    require(set(modules) == expected, 'Expected M00–M19 and O01–O12')
    order = graph['execution_order']
    require(len(order) == len(expected) and set(order) == expected, 'Execution order must contain each module once')
    position = {mid: i for i, mid in enumerate(order)}
    master = read(ROOT / 'BBSBOX_MASTER_SPEC.md')
    coverage = read(DEV / 'COVERAGE.md')
    decisions = read(DEV / 'DECISIONS.md')
    index = read(DEV / 'README.md')
    ledger = read(DEV / 'WORK_LEDGER.md')
    review = read(DEV / 'REVIEW-2026-10-25.md')
    start = read(DEV / 'START_HERE.md')
    contracts = {p.name[:3]: p for p in (DEV / 'contracts').glob('C*.md')}
    require(len(contracts) == 5, 'Expected five shared contracts')
    index_rows = {re.search(r'\b[MO]\d{2}\b', r[0]).group(): r for r in rows(index)
                  if re.search(r'\[([MO]\d{2}) —', r[0])}
    sizes = []
    for mid, m in modules.items():
        path = ROOT / m['path']
        require(path.is_file(), f'{mid}: missing brief')
        if not path.is_file():
            continue
        brief = read(path)
        for dep in m['deps']:
            require(dep in modules, f'{mid}: unknown dependency {dep}')
            require(position.get(dep, 999) < position.get(mid, -1), f'{mid}: order/cycle error at {dep}')
        pre = re.search(r'\*\*Prerequisites:\*\*([^\n]+)', brief)
        require(pre is not None and ids(pre.group(), '[MO]') == set(m['deps']), f'{mid}: brief prerequisites differ from graph')
        row = index_rows.get(mid)
        col = 2 if mid.startswith('M') else 1
        require(row is not None and ids(row[col], '[MO]') == set(m['deps']), f'{mid}: index prerequisites differ from graph')
        for ref in m['refs']:
            require(ref in brief and ref in master, f'{mid}: missing source {ref}')
        require(set(m['contracts']) <= set(contracts), f'{mid}: missing contract')
        packet = brief.split('## Read only this packet', 1)[1].split('## Product outcome', 1)[0]
        require(ids(packet, 'C') == set(m['contracts']), f'{mid}: reading packet contracts differ')
        require(ids(packet, 'T') == set(m['decisions']), f'{mid}: decision lookups differ')
        for decision in m['decisions']:
            require(re.search(r'^\| ' + decision + r' \|', decisions, re.M), f'{mid}: unknown decision {decision}')
        require(m['gate'] in brief, f'{mid}: missing gate')
        require(f'| {mid} |' in review, f'{mid}: no individual review record')
        require(re.search(r'\[' + mid + r'\].*\| Approved \| Unstarted \|', ledger), f'{mid}: ledger status is missing/inconsistent')
        text = start + '\n' + brief + '\n' + '\n'.join(read(contracts[c]) for c in m['contracts'])
        count = len(text.split())
        sizes.append((count, mid))
        require(count <= 2500, f'{mid}: reading packet is {count} words, over 2500')
        require(graph['version'] in brief and graph['tier1_version'] in brief, f'{mid}: baseline versions differ')
    coverage_rows = {r[0].strip(): r for r in rows(coverage)}
    for prefix, number in [('R', 28), ('P', 24), ('AC', 34), ('D', 7), ('N', 12)]:
        for i in range(1, number + 1):
            ref = f'{prefix}{i:02}'
            require(ref in master, f'Missing Tier 1 ID {ref}')
            row = coverage_rows.get(ref)
            require(row is not None, f'Missing coverage row {ref}')
            if prefix in ('R', 'P', 'AC'):
                owners = {mid for mid, m in modules.items() if ref in m['refs']}
                require(bool(owners), f'{ref}: no evaluating module')
                require(row is not None and ids(row[1], '[MO]') == owners, f'{ref}: coverage owners differ from graph')
    require(ids(decisions.split('## Acceptance record')[0], 'T') == {f'T{i:02}' for i in range(1, 15)}, 'Technical decisions incomplete')
    # Validate document links, including local fragments, without fetching external sites.
    documents = sorted(ROOT.glob('*.md')) + sorted((ROOT / 'docs').rglob('*.md'))
    links = 0
    for path in documents:
        text = read(path)
        for match in re.finditer(r'\[[^\]\n]*\]\(([^)\n]+)\)', text):
            target = match.group(1).strip('<>')
            url = urlsplit(target)
            if url.scheme or target.startswith('//'):
                continue
            dest = (path.parent / unquote(url.path)).resolve() if url.path else path
            links += 1
            require(dest.is_file(), f'{path.relative_to(ROOT)}: broken link {target}')
            if url.fragment and dest.is_file() and dest.suffix == '.md':
                headings = re.findall(r'^#+\s+(.+)$', read(dest), re.M)
                anchors = {re.sub(r'[^\w\- ]', '', h.lower()).replace(' ', '-') for h in headings}
                require(unquote(url.fragment) in anchors, f'{path.relative_to(ROOT)}: unknown fragment {target}')
    for name, title in [('AGPL-3.0-or-later', 'GNU AFFERO GENERAL PUBLIC LICENSE'), ('CC-BY-SA-4.0', 'Attribution-ShareAlike 4.0 International')]:
        path = ROOT / f'LICENSES/{name}.txt'
        require(path.is_file() and title in read(path), f'Missing/mismatched license text {name}')
    if errors:
        print('\n'.join('ERROR: ' + error for error in errors))
        return 1
    largest, mid = max(sizes)
    print(f'PASS: {len(modules)} modules; acyclic ordered dependencies; 28 R / 24 P / 34 AC / 7 D / 12 N coverage; 14 decisions; {len(documents)} Markdown files; {links} local links.')
    print(f'PASS: brief/index/graph/ledger/review agreement; five contracts; license texts. Largest reading packet: {mid}, {largest}/2500 words (excluding targeted decision-row lookups).')
    print('Static document checks only. No application, device, encryption or runtime tests executed.')
    return 0


if __name__ == '__main__':
    sys.exit(validate())
