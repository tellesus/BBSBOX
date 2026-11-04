// SPDX-License-Identifier: AGPL-3.0-or-later
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import ts from 'typescript';

const pages = [
  ['index.html', 'home.html', ['NODE_NAME', 'WELCOME']],
  ['login/index.html', 'login.html', ['NODE_NAME', 'LOGIN_MESSAGE']],
  ['account/index.html', 'account.html', ['USERNAME', 'ROLE', 'CSRF', 'SYSOP_LINK']],
  ['settings/theme/index.html', 'theme.html', ['THEME_STATUS', 'THEME_REVISION', 'CSRF', 'BACKGROUND', 'TEXT', 'ACCENT', 'SECONDARY']],
  ['settings/site/index.html', 'site.html', ['SITE_STATUS', 'SITE_REVISION', 'CSRF', 'MESSAGE', 'ART', 'MENU_OPTIONS']],
  ['settings/accounts/index.html', 'accounts.html', ['ACCOUNT_STATUS', 'CSRF']],
  ['discussions/index.html', 'discussions.html', ['DISCUSSIONS_TITLE', 'DISCUSSIONS_CONTENT']],
  ['mail/index.html', 'mail.html', ['MAIL_TITLE', 'MAIL_CONTENT']],
  ['directory/index.html', 'directory.html', ['DIRECTORY_TITLE', 'DIRECTORY_CONTENT']],
  ['feeds/index.html', 'feeds.html', ['FEED_TITLE', 'FEED_CONTENT']],
  ['events/index.html', 'events.html', ['EVENTS_TITLE', 'EVENTS_CONTENT']],
  ['groups/index.html', 'groups.html', ['GROUPS_TITLE', 'GROUPS_CONTENT']],
  ['files/index.html', 'files.html', ['FILES_TITLE', 'FILES_CONTENT']],
  ['games/index.html', 'games.html', ['GAMES_CONTENT']],
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

const gameSource = await readFile('../../games/demo/web/game.ts', 'utf8');
const gameScript = ts.transpileModule(gameSource, {
  compilerOptions: { target: ts.ScriptTarget.ES2022, strict: true, newLine: ts.NewLineKind.LineFeed }
}).outputText;
const gameTarget = '../../games/demo/assets/game.js';
if (verify) {
  if (gameScript !== (await readFile(gameTarget, 'utf8')).replaceAll('\r\n', '\n')) throw new Error(`Rebuild ${gameTarget}`);
} else {
  await writeFile(gameTarget, gameScript);
}
