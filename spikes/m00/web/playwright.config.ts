// SPDX-License-Identifier: AGPL-3.0-or-later
import { defineConfig } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { createHash, X509Certificate } from 'node:crypto';
const leaf = new X509Certificate(readFileSync(new URL('../.state/tls/server.pem', import.meta.url)));
const pin = createHash('sha256').update(leaf.publicKey.export({ type: 'spki', format: 'der' })).digest('base64');
export default defineConfig({
  testDir: './tests', workers: 1, retries: 0, timeout: 30_000,
  use: {
    baseURL: 'https://127.0.0.1:9443',
    channel: process.env.BBSBOX_BROWSER_CHANNEL || undefined,
    launchOptions: { args: [`--ignore-certificate-errors-spki-list=${pin}`, '--disable-background-networking'] },
    screenshot: 'only-on-failure', trace: 'retain-on-failure'
  },
  reporter: [['list'], ['json', { outputFile: 'test-results/results.json' }]]
});
