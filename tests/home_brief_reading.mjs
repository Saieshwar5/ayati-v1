import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { writeFile } from 'node:fs/promises';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { root } from './fixtures/goose_app.mjs';

const { chromium } = createRequire(join(root, 'web/package.json'))('playwright');
test('personal briefs show short findings and keep long content keyboard-readable in a bounded panel', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 150000,
}, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true,
    capacity: 1, workspaceLimit: 1, modelLimit: 1, leaseSeconds: 15 });
  await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  const short = randomUUID(), long = randomUUID(), metric = randomUUID();
  const body = 'Check the pending delivery question.';
  const longBody = ('A saved finding stays in its personal panel. '.repeat(14)) + 'END_OF_SAVED_BRIEF';
  for (const [id, title, template, section] of [
    [short, 'Recovery priorities', 'brief', { kind: 'text', heading: 'Next step', body }],
    [long, 'Detailed personal findings', 'brief', { kind: 'text', heading: 'Saved observations', body: longBody }],
    [metric, 'Open questions', 'metric', { kind: 'metric', label: 'Pending', value: '2' }],
  ]) {
    const prompt = `Prepare my ${title} panel.`;
    app.provider.setControl(prompt, { steps: [{ tool: 'save_home_panel', arguments: {
      id, expected_revision: 0, title, purpose: 'A personal recovery check', template, sections: [section], sources: [],
    } }], checkpoint: 'Your draft is ready for review.' });
    await app.done(await app.send(prompt));
    await app.request(`${app.path}/home-panels/${id}/review`, app.cookie, { request: randomUUID(), revision: 1 });
  }
  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  const [name, value] = app.cookie.split('=');
  await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
  const page = await context.newPage(), errors = [];
  page.on('pageerror', error => errors.push(error.message));
  t.after(async () => {
    if (!t.passed) await page.screenshot({ path: join(app.data, 'brief-failure.png') }).catch(() => {});
    await browser.close();
  });
  const workspace = app.path.split('/').at(-1);
  await page.goto(`${app.base}/w/${workspace}/?view=home`);
  const card = id => page.locator(`[data-panel="${id}"]`);
  await card(short).waitFor();
  const requests = app.provider.requests.length;
  async function shortVisible() {
    await card(short).scrollIntoViewIfNeeded();
    const visibility = await card(short).evaluate(node => {
      const viewport = node.querySelector('.panel-snapshot').getBoundingClientRect();
      const content = node.querySelector('p.panel-text').getBoundingClientRect();
      return { text: node.querySelector('p.panel-text').textContent,
        top: content.top >= viewport.top, bottom: content.bottom <= viewport.bottom + 1 };
    });
    assert.equal(visibility.text, body);
    assert.equal(visibility.top && visibility.bottom, true, 'A short brief must show its main sentence without inner scrolling');
  }
  await shortVisible();
  assert.ok((await card(short).boundingBox()).height > (await card(metric).boundingBox()).height,
    'Text briefs need more reading room than one metric');
  await page.screenshot({ path: join(app.data, 'brief-desktop.png') });
  const region = card(long).getByRole('region', { name: 'Detailed personal findings saved content', exact: true });
  await region.focus();
  const outerBefore = await page.evaluate(() => document.scrollingElement.scrollTop);
  await page.keyboard.press('End');
  await page.waitForFunction(id => {
    const node = document.querySelector(`[data-panel="${id}"] .panel-snapshot`);
    return node.scrollTop > 0 && node.scrollTop + node.clientHeight >= node.scrollHeight - 2;
  }, long);
  assert.equal(await page.evaluate(() => document.scrollingElement.scrollTop), outerBefore);
  await card(long).getByRole('button', { name: 'Sources and details', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Saved panel details', exact: true });
  assert.equal(await dialog.locator('p.panel-text').textContent(), longBody);
  await page.keyboard.press('Escape');
  await dialog.waitFor({ state: 'detached' });
  assert.equal(await card(long).getByRole('button', { name: 'Sources and details', exact: true })
    .evaluate(node => node === document.activeElement), true);
  for (const [width, height, dark] of [[720, 640, false], [390, 844, false], [390, 844, true]]) {
    await page.setViewportSize({ width, height });
    await page.evaluate(dark => { document.documentElement.dataset.theme = dark ? 'dark' : 'light'; }, dark);
    await shortVisible();
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
    await page.screenshot({ path: join(app.data, `brief-${width}-${dark ? 'dark' : 'light'}.png`) });
  }
  await page.reload();
  await card(short).waitFor();
  await shortVisible();
  assert.equal(app.provider.requests.length, requests, 'Reading and resizing saved panels must not call a model');
  assert.deepEqual(errors, []);
  await writeFile(join(app.data, 'brief-reading.json'), JSON.stringify({ passed: true,
    short_body_visible: true, long_body_keyboard_scroll: true, metric_compact: true,
    sizes: [1280, 720, 390], themes: ['light', 'dark'], extra_inference: 0 }, null, 2));
  app.pass('Short saved briefs are visible; long content scrolls by keyboard, details restore focus, compact metrics and mobile/dark layouts survive reload without inference');
});
