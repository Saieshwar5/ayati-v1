import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { setup, root } from './fixtures/goose_app.mjs';
const { chromium } = createRequire(join(root, 'web/package.json'))('playwright');

test('empty Home contains its welcome content above agents and footer', { timeout: 60000 }, async t => {
  const app = await setup(t, false, { AYATI_BROWSER: '0' });
  const browser = await chromium.launch(); t.after(() => browser.close());
  const page = await browser.newPage();
  await page.goto(app.base.replace('/api', ''));
  await page.locator('.home-welcome').waitFor();
  await page.locator('#home-agents .home-card').waitFor();
  for (const width of [1886, 1280, 760, 561, 390, 320]) {
    await page.setViewportSize({ width, height: 1000 });
    const layout = await page.evaluate(() => {
      const welcome = document.querySelector('.home-welcome');
      const box = welcome.getBoundingClientRect();
      return {
        contained: [...welcome.children].every(child => {
          const rect = child.getBoundingClientRect();
          return rect.top >= box.top && rect.bottom <= box.bottom && rect.right <= box.right;
        }),
        separated: document.querySelector('.home-team').getBoundingClientRect().top >= box.bottom,
        footer: document.querySelector('.home-footer').getBoundingClientRect().top >= document.querySelector('.home-team').getBoundingClientRect().bottom,
        fits: document.documentElement.scrollWidth <= innerWidth,
      };
    });
    assert.deepEqual(layout, { contained: true, separated: true, footer: true, fits: true }, `${width}px`);
    if (width === 1886 || width === 390) await page.screenshot({ path: `/tmp/ayati-home-empty-${width}.png`, fullPage: true });
  }
});
