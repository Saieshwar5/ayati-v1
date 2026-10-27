import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { mkdir, writeFile } from 'node:fs/promises';
import { setup, until, root } from './fixtures/goose_app.mjs';
const { chromium } = createRequire(join(root, 'web/package.json'))('@playwright/test');

test('adopted Home accepts general families and private creation keeps general type, chart style and draft without starting work', {
  timeout: 120000,
}, async t => {
  const app = await setup(t, false, { AYATI_BROWSER: '0' });
  const browser = await chromium.launch({ headless: true }); t.after(() => browser.close());
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  const base = app.base.replace('/api', ''), errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto(base);
  await page.getByRole('button', { name: 'Add panel', exact: false }).click();
  await page.getByRole('dialog').locator('a[href*="agent=local-agent"]').click();
  await page.getByLabel('Panel type', { exact: true }).waitFor();
  assert.equal(await page.getByLabel('Panel type', { exact: true }).locator('option').count(), 8);
  await page.getByLabel('Panel type', { exact: true }).selectOption('chart');
  await page.getByLabel('Chart style', { exact: true }).selectOption('donut');
  await page.getByRole('textbox', { name: /^Message / }).fill('My unsent general panel request'); await page.reload();
  assert.equal(await page.getByLabel('Panel type', { exact: true }).inputValue(), 'chart');
  assert.equal(await page.getByLabel('Chart style', { exact: true }).inputValue(), 'donut');
  assert.equal(await page.getByRole('textbox', { name: /^Message / }).inputValue(), 'My unsent general panel request');
  assert.equal(app.provider.requests.length, 0);
  await page.getByRole('button', { name: 'Exit creation mode' }).click();
  assert.equal(await page.getByRole('textbox', { name: /^Message / }).inputValue(), 'My unsent general panel request');
  const sections = [
    { kind: 'metric', label: 'A', value: '12', comparison: { label: 'Previous', value: 8, favorable: 'increase' } },
    { kind: 'table', columns: ['Item', 'Count'], column_types: ['text', 'number'], rows: [['A', '100'], ['B', '2']] },
    { kind: 'collection', items: [{ id: 'a', title: '<img src=x onerror=alert(1)>', status: 'Active' }] },
    { kind: 'progress', items: [{ id: 'a', label: 'Goal', actual: 72, target: 100 }] },
    { kind: 'timeline', events: [{ id: 'a', at_ms: 1000, label: 'Changed' }] },
    { kind: 'text', heading: 'Context', body: 'Illustrative data, not a live business result.' },
    { kind: 'chart', title: 'Distribution', variant: 'donut', points: [{ label: 'A', value: 2 }, { label: 'B', value: 3 }] },
  ];
  const ids = [];
  for (const [i, section] of sections.entries()) {
    const panel = { id: randomUUID(), expected_revision: 0, title: `Reusable ${i+1}`, purpose: 'Local fixture',
      publish: true, sections: [section], sources: [] };
    const task = await app.submit('workspace '+JSON.stringify({ tool: 'panel_write', arguments: panel }));
    const permission = await until(async () => (await app.state()).permissions[0], 'Exact publication approval missing');
    await app.request(`/permissions/${permission.id}`, { allow: true });
    assert.equal((await app.done(task.id)).task.status, 'completed'); ids.push(panel.id);
  }
  await page.goto(base);
  await until(async () => await page.locator('.home-panel-card').count() === 7, 'Typed local snapshots did not load');
  const card = i => page.locator(`[data-panel="${ids[i]}"]`);
  assert.equal(await card(0).locator('.panel-comparison').getAttribute('data-sentiment'), 'favorable');
  await card(1).getByRole('button', { name: 'Count', exact: true }).click();
  assert.deepEqual(await card(1).locator('tbody td:nth-child(2)').allTextContents(), ['2', '100']);
  assert.equal(await card(2).locator('img').count(), 0);
  assert.equal(await card(3).getByRole('progressbar', { name: 'Goal' }).getAttribute('aria-valuenow'), '72');
  assert.equal(await card(4).locator('time').count(), 1);
  assert.equal(await card(6).locator('svg circle').count(), 2);
  const requests = app.provider.requests.length;
  await page.reload(); await card(6).waitFor(); assert.equal(app.provider.requests.length, requests);
  for (const width of [1280, 390, 320]) {
    await page.setViewportSize({ width, height: 850 });
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
  }
  assert.deepEqual(errors, []);
  await mkdir(join(root, '.tmp/general-home-panels'), { recursive: true });
  await writeFile(join(root, '.tmp/general-home-panels/local-live-result.json'), JSON.stringify({ passed: true,
    families: 7, source: 'Real Goose with scripted local inference', typeChoices: 7, chartChoices: 5,
    draftPreserved: true, safeMarkup: true, numericSort: true, requestsOnRead: 0 }, null, 2));
});
