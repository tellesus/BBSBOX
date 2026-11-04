// SPDX-License-Identifier: AGPL-3.0-or-later
import { mkdir, readFile, writeFile } from 'node:fs/promises';

const pages = [
  ['index.html', 'home.html', ['NODE_NAME', 'WELCOME']],
  ['login/index.html', 'login.html', ['NODE_NAME', 'LOGIN_MESSAGE']],
  ['account/index.html', 'account.html', ['USERNAME', 'ROLE', 'CSRF', 'SYSOP_LINK']],
  ['settings/theme/index.html', 'theme.html', ['THEME_STATUS', 'THEME_REVISION', 'CSRF', 'BACKGROUND', 'TEXT', 'ACCENT', 'SECONDARY']],
  ['settings/site/index.html', 'site.html', ['SITE_STATUS', 'SITE_REVISION', 'CSRF', 'MESSAGE', 'ART', 'MENU_OPTIONS']],
  ['settings/accounts/index.html', 'accounts.html', ['ACCOUNT_STATUS', 'CSRF']],
  ['unavailable/index.html', 'unavailable.html', ['PAGE_TITLE', 'PAGE_MESSAGE']]
];

const verify = process.argv.includes('--check');
if (!verify) await mkdir('compiled', { recursive: true });
for (const [source, destination, pageSlots] of pages) {
  const slots = ['NAV', ...pageSlots];
  const html = (await readFile(`build/${source}`, 'utf8')).replaceAll('\r\n', '\n');
  const found = [...html.matchAll(/__BBSBOX_([A-Z_]+)__/g)].map(match => match[1]);
  if (found.length !== slots.length || slots.some(slot => found.filter(name => name === slot).length !== 1)) {
    throw new Error(`Unexpected template slots in ${source}`);
  }
  const script = '<script src="/assets/shell-state.js" defer></script>';
  const withoutState = html.replace(script, '');
  if (withoutState === html || /<script\b|\sstyle=|(?:href|src)="(?:https?:|\/\/|data:)/i.test(withoutState)) {
    throw new Error(`Unexpected executable, inline style or remote asset in ${source}`);
  }
  const target = `compiled/${destination}`;
  if (verify) {
    if (html !== (await readFile(target, 'utf8')).replaceAll('\r\n', '\n')) throw new Error(`Rebuild ${target}`);
  } else {
    await writeFile(target, html);
  }
}
