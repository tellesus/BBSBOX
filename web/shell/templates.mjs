// SPDX-License-Identifier: AGPL-3.0-or-later
import { mkdir, readFile, writeFile } from 'node:fs/promises';

const pages = [
  ['index.html', 'home.html', ['NODE_NAME']],
  ['login/index.html', 'login.html', ['NODE_NAME', 'LOGIN_MESSAGE']],
  ['account/index.html', 'account.html', ['USERNAME', 'ROLE', 'CSRF']]
];

const verify = process.argv.includes('--check');
if (!verify) await mkdir('compiled', { recursive: true });
for (const [source, destination, slots] of pages) {
  const html = (await readFile(`build/${source}`, 'utf8')).replaceAll('\r\n', '\n');
  const found = [...html.matchAll(/__BBSBOX_([A-Z_]+)__/g)].map(match => match[1]);
  if (found.length !== slots.length || slots.some(slot => found.filter(name => name === slot).length !== 1)) {
    throw new Error(`Unexpected template slots in ${source}`);
  }
  if (/<script\b|\sstyle=|(?:href|src)="(?:https?:|\/\/|data:)/i.test(html)) {
    throw new Error(`Unexpected executable, inline style or remote asset in ${source}`);
  }
  const target = `compiled/${destination}`;
  if (verify) {
    if (html !== (await readFile(target, 'utf8')).replaceAll('\r\n', '\n')) throw new Error(`Rebuild ${target}`);
  } else {
    await writeFile(target, html);
  }
}
