// SPDX-License-Identifier: AGPL-3.0-or-later
import { test, expect } from '@playwright/test';

const screen = new URL('../../native/ui/index.html', import.meta.url).href;

test('opening the HTML file explains the desktop requirement without crashing', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto(screen);
  await expect(page.locator('#status')).toContainText('requires the BBSBOX desktop app');
  await expect(page.locator('#launch-help')).toContainText('Start BBSBOX.cmd');
  await expect(page.locator('#identity')).toBeHidden();
  expect(errors).toEqual([]);
});

test('the HTML launch instructions also work without JavaScript', async ({ browser }) => {
  const context = await browser.newContext({ javaScriptEnabled: false });
  try {
    const page = await context.newPage();
    await page.goto(screen);
    await expect(page.locator('#launch-help')).toBeVisible();
    await expect(page.locator('#identity')).toBeHidden();
  } finally {
    await context.close();
  }
});

for (const create of [false, true]) {
  test(`${create ? 'create' : 'unlock'} shows progress, prevents repeat clicks and recovers from an error`, async ({ page }) => {
    // UI transport fixture only; check.py separately exercises the actual native crypto/storage.
    await page.addInitScript(() => {
      Object.assign(window, {
        __requests: [],
        __TAURI__: { core: { invoke: (command: string, arguments_: unknown) => {
          if (command === 'probe_status') return new Promise(resolve => {
            Object.assign(window, { __start: resolve });
          });
          (window as any).__requests.push({ command, arguments_ });
          return new Promise((resolve, reject) => Object.assign(window, { __done: resolve, __fail: reject }));
        } } },
      });
    });
    await page.goto(screen);
    await expect(page.locator('#status')).toHaveText('Starting desktop client…');
    await expect(page.locator('#identity')).toBeHidden();
    await page.evaluate(() => (window as any).__start({ person: null, checking: false }));
    const form = page.locator('#identity');
    const button = page.locator(create ? '#create' : 'button[type="submit"]');
    await expect(form).toBeVisible();
    await page.locator('#passphrase').fill('native UI fixture passphrase');
    await button.hover();
    await page.mouse.down();
    await expect.poll(() => button.evaluate(element => getComputedStyle(element).transform)).not.toBe('none');
    await page.mouse.up();
    await expect(form).toHaveAttribute('aria-busy', 'true');
    await expect(button).toBeDisabled();
    await expect(page.locator('#passphrase')).toHaveValue('');
    await expect(page.locator('#status')).toHaveText(create ? 'Creating identity…' : 'Unlocking identity…');
    // Even a second dispatched form event cannot submit another operation while the first waits.
    await form.dispatchEvent('submit');
    expect(await page.evaluate(() => (window as any).__requests.length)).toBe(1);
    expect(await page.evaluate(() => (window as any).__requests[0].arguments_.create)).toBe(create);
    await page.evaluate(() => (window as any).__fail('Fixture passphrase error'));
    await expect(page.locator('#status')).toHaveText('Fixture passphrase error');
    await expect(form).toHaveAttribute('aria-busy', 'false');
    await expect(button).toBeEnabled();
    await expect(page.locator('#passphrase')).toBeFocused();
    await page.locator('#passphrase').fill('native UI retry passphrase');
    await button.click();
    await expect(form).toHaveAttribute('aria-busy', 'true');
    await page.evaluate(() => (window as any).__done({ person: 'person:fixture', requires_rejoin: false }));
    await expect(page.locator('#status')).toHaveText('Identity unlocked.');
    await expect(form).toBeHidden();
  });
}

test('a native startup failure gives a visible error instead of an endless starting message', async ({ page }) => {
  await page.addInitScript(() => Object.assign(window, {
    __TAURI__: { core: { invoke: () => Promise.reject('Fixture connection failure') } },
  }));
  await page.goto(screen);
  await expect(page.locator('#status')).toContainText('Client could not start: Fixture connection failure');
  await expect(page.locator('#status')).not.toHaveClass(/working/);
  await expect(page.locator('#identity')).toBeHidden();
});
