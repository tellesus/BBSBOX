// SPDX-License-Identifier: AGPL-3.0-or-later
import { test, expect } from '@playwright/test';
import { spawn, type ChildProcess } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash, createPublicKey, verify } from 'node:crypto';
import https from 'node:https';
import { Decoder, encode } from 'cbor-x';
const decoder = new Decoder({ mapsAsObjects: false });
const decode = (bytes: Uint8Array) => decoder.decode(bytes);

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const binary = process.env.BBSBOX_TEST_BINARY || join(root, 'node/target/debug', process.platform === 'win32' ? 'bbsbox-foundation.exe' : 'bbsbox-foundation');
const ca = readFileSync(join(root, '.state/tls/ca.pem'));
let server: ChildProcess;
let state: string;
let output = '';
function request(path: string, trust = true): Promise<any> {
  return new Promise((accept, reject) => {
    const req = https.get(`https://127.0.0.1:9443${path}`, { ...(trust ? { ca } : {}), rejectUnauthorized: true }, response => {
      let text = ''; response.on('data', chunk => { text += chunk; });
      response.on('end', () => { try { accept(JSON.parse(text)); } catch (e) { reject(e); } });
    });
    req.setTimeout(1000, () => req.destroy(new Error('request timeout')));
    req.on('error', reject);
  });
}
async function start() {
  server = spawn(binary, [], { cwd: root, windowsHide: true, env: { ...process.env, BBSBOX_STATE: state, BBSBOX_BIND: '127.0.0.1:9443' }, stdio: ['ignore','pipe','pipe'] });
  server.stdout?.on('data', chunk => { output += chunk; }); server.stderr?.on('data', chunk => { output += chunk; });
  server.on('error', e => { output += e.message; });
  for (let attempt = 0; attempt < 100; attempt++) {
    try { await request('/api/node'); return; } catch { await new Promise(r => setTimeout(r, 100)); }
    if (server.exitCode !== null) throw new Error(`Node exited: ${output}`);
  }
  throw new Error(`Node did not start: ${output}`);
}
async function stop() {
  if (!server || server.exitCode !== null) return;
  await new Promise<void>(resolve => { server.once('exit', () => resolve()); server.kill(); });
}
test.beforeAll(async () => {
  state = mkdtempSync(join(root, '.state/e2e-')); mkdirSync(join(state, 'tls'));
  for (const name of ['ca.pem','server.pem','server-key.pem']) copyFileSync(join(root,'.state/tls',name),join(state,'tls',name));
  await start();
});
test.afterAll(stop);

test('TLS accepts the local CA and rejects an untrusted connection', async () => {
  expect((await request('/api/node')).stage).toBe('M00 experiment');
  await expect(request('/api/node', false)).rejects.toThrow();
});

test('local browser journey, literal text, retry and independent COSE verification', async ({ page }) => {
  const outside: string[] = []; const failures: string[] = [];
  page.on('pageerror', error => failures.push(error.message));
  await page.route('**/*', route => {
    const url = new URL(route.request().url());
    if (url.protocol === 'https:' && url.origin === 'https://127.0.0.1:9443') return route.continue();
    outside.push(url.href); return route.abort();
  });
  await page.goto('/');
  await expect(page.getByText('Local node connected')).toBeVisible();
  await expect(page.getByText('A little quiet, for now.')).toBeVisible();
  await page.screenshot({ path: 'test-results/preview.png', fullPage: true });
  await page.getByLabel('Your name').fill('Test neighbor');
  const text = 'Hello, neighbors! <script>window.unwanted = true</script>';
  await page.getByLabel('Your message', { exact: true }).fill(text);
  await page.getByRole('button', { name: 'Leave your message' }).click();
  await expect(page.getByText('Saved here. Your message is signed and verified.')).toBeVisible();
  await expect(page.locator('.message-body')).toHaveText(text);
  expect(await page.evaluate(() => (window as any).unwanted)).toBeUndefined();
  await page.reload(); await expect(page.locator('.message-body')).toHaveText(text);
  const records = await request('/api/messages'); const record = records[0]; const info = await request('/api/node');
  const bytes = Buffer.from(record.envelope_hex, 'hex'); const body = Buffer.from(record.body_hex, 'hex');
  const [protectedHeader, unprotected, payload, signature] = decode(bytes);
  expect(decode(protectedHeader).get(1)).toBe(-8);
  expect(unprotected.size).toBe(0);
  const commitment = decode(payload);
  expect(commitment[0]).toBe('bbsbox.m00.event/v0');
  expect(Buffer.from(commitment[13]).toString('hex')).toBe(createHash('sha256').update(body).digest('hex'));
  expect(commitment[14]).toBe(body.length);
  expect(decode(body)).toEqual(['Test neighbor', text]);
  expect(createHash('sha256').update(bytes).digest('hex')).toBe(record.id);
  const spki = Buffer.concat([Buffer.from('302a300506032b6570032100','hex'),Buffer.from(info.public_key,'hex')]);
  const publicKey = createPublicKey({ key: spki, format: 'der', type: 'spki' });
  const signingInput = encode(['Signature1',protectedHeader,Buffer.from('bbsbox.m00.event/v0'),payload]);
  expect(verify(null, signingInput, publicKey, signature)).toBe(true);
  const corrupt = Buffer.from(signingInput); corrupt[corrupt.length-1] ^= 1;
  expect(verify(null, corrupt, publicKey, signature)).toBe(false);
  expect(failures).toEqual([]); expect(outside).toEqual([]);
  await page.evaluate(() => { document.documentElement.style.scrollBehavior = 'auto'; window.scrollTo(0,0); });
  await expect.poll(() => page.evaluate(() => window.scrollY)).toBe(0);
  await page.screenshot({ path: 'test-results/desktop.png', fullPage: true });
});

test('narrow layout stays usable and draft survives a failed send', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 }); await page.goto('/');
  await expect(page.getByText('Local node connected')).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.getByLabel('Your name').fill('Another neighbor');
  await page.getByLabel('Your message', { exact: true }).fill('Keep my draft');
  await page.route('**/api/messages', route => route.request().method() === 'POST' ? route.abort() : route.continue());
  await page.getByRole('button', { name: 'Leave your message' }).click();
  await expect(page.getByRole('alert')).toBeVisible();
  await expect(page.getByLabel('Your message', { exact: true })).toHaveValue('Keep my draft');
  await page.unroute('**/api/messages');
  await page.getByRole('button', { name: 'Leave your message' }).click();
  await expect(page.getByText('Saved here. Your message is signed and verified.')).toBeVisible();
  await page.evaluate(() => { document.documentElement.style.scrollBehavior = 'auto'; window.scrollTo(0,0); });
  await expect.poll(() => page.evaluate(() => window.scrollY)).toBe(0);
  await page.screenshot({ path: 'test-results/mobile-layout.png', fullPage: true });
});

test('retry IDs deduplicate and a cold process restart preserves state and key', async ({ page }) => {
  await page.goto('/'); await expect(page.getByText('Local node connected')).toBeVisible();
  const before = await request('/api/node');
  const results = await page.evaluate(async () => {
    const data = { request_id: 'network-retry-fixture', label: 'Retry fixture', text: 'Exactly one local record' };
    const send = () => fetch('/api/messages', { method: 'POST', headers: { 'Content-Type':'application/json' }, body: JSON.stringify(data) }).then(r=>r.json());
    return Promise.all(Array.from({ length: 8 }, send));
  });
  expect(new Set(results.map(result => result.id)).size).toBe(1);
  expect((await request('/api/node')).messages).toBe(before.messages + 1);
  const publicKey = before.public_key;
  await stop(); await start();
  expect((await request('/api/node')).public_key).toBe(publicKey);
  expect((await request('/api/node')).messages).toBe(before.messages + 1);
  expect((await request('/api/messages'))[0].id).toBe(results[0].id);
});
