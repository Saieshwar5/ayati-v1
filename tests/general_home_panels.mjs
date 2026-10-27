import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { mkdtemp, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { remoteControlMcp } from './fixtures/remote_control_mcp.mjs';
import { until, root } from './fixtures/goose_app.mjs';
const { chromium } = createRequire(join(root, 'web/package.json'))('@playwright/test');

test('general panel families and chart variants survive real managed draft/review, sorting, narrow themes and source revocation', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 180000,
}, async t => {
  const remote = await remoteControlMcp(); t.after(() => remote.close());
  const dir = await mkdtemp(join(root, '.tmp/general-panel-profile-'));
  const profiles = join(dir, 'servers.json'), bearer = join(dir, 'bearer');
  await writeFile(profiles, '[]', { mode: 0o600 }); await writeFile(bearer, remote.token, { mode: 0o600 });
  const app = await workerFixture(t, { mode: 'pooled', control: true, capacity: 1, workspaceLimit: 1, modelLimit: 1,
    leaseSeconds: 15, apiEnv: { AYATI_MCP_SERVERS_FILE: profiles } });
  const workspace = app.path.split('/').at(-1), member = await app.login(1);
  const person = (await app.request('/api/identity/session', member)).value.user;
  await app.request(`${app.path}/chat-grants`, app.cookie, { user: person.id, agent: app.profile.id, allowed: true });
  const chat = (await app.request(`${app.path}/chats`, app.cookie, { id: randomUUID(), kind: 'channel',
    name: 'General panel review', users: [person.id], agents: [app.profile.id] })).value.id;
  const connection = randomUUID(), control = `${app.path}/agent-profiles/${app.profile.id}/control`;
  await writeFile(profiles, JSON.stringify([{ id: 'orders', workspace, name: 'Orders', account: 'orders@example.test',
    url: remote.url, bearer_file: bearer, operations: remote.operations }]));
  await app.request(`${app.path}/control/systems`, app.cookie, { server: 'orders', approved: true, revision: 0 });
  await app.request(`${app.path}/control/connections`, app.cookie, { id: connection, server: 'orders', enabled: true, revision: 0 });
  await app.request(`${control}/rules`, app.cookie, { connection, operation: 'order.read', mode: 'allow', revision: 0, chat });
  const raw = [
    { kind: 'metric', label: 'Measure', value: '12,480', comparison: { label: 'Previous period', value: -8.4, unit: '%', favorable: 'decrease' } },
    { kind: 'metrics', items: [{ id: 'a', label: 'A', value: '12,480' }, { id: 'b', label: 'B', value: '76%' }, { id: 'c', label: 'C', value: '320' }] },
    { kind: 'table', columns: ['Item', 'Value'], column_types: ['text', 'number'], rows: [['A', '100'], ['B', '2'], ['C', '10']] },
    { kind: 'collection', items: [{ id: 'a', title: 'INERT_ITEM <img src=x onerror=alert(1)>', description: 'Supporting description', status: 'Active' }] },
    { kind: 'progress', items: [{ id: 'a', label: 'Measure A', actual: 72, target: 100 }, { id: 'b', label: 'Measure B', actual: 125, target: 100 }] },
    { kind: 'timeline', events: [{ id: 'a', at_ms: 1793089200000, label: 'Earlier' }, { id: 'b', at_ms: 1793092800000, label: 'Later' }] },
    { kind: 'text', heading: 'Context', body: 'Illustrative findings from a scripted local source.\nA supporting point.' },
    ...['line', 'bar', 'area', 'stacked_bar', 'donut'].map(variant => ({ kind: 'chart', title: 'Chart '+variant, variant,
      series: (variant === 'donut' ? ['A'] : ['A', 'B']).map((name, i) => ({ name,
        points: [{ label: 'Mon', value: 2+i }, { label: 'Tue', value: 4+i }, { label: 'Wed', value: 3+i }] })), unit: 'units' })),
  ];
  const drafts = raw.map((section, i) => ({ id: randomUUID(), expected_revision: 0, title: `General view ${i+1}`,
    purpose: 'Illustrative data for local UI verification', sections: [section], sources: [] }));
  const command = 'Prepare these twelve illustrative general panel drafts for my review, using the reviewed source. No schedule.';
  app.provider.setControl(command, { steps: [
    { tool: 'request_action', arguments: { connection, operation: 'order.read', arguments: { order: 'one' } } },
    ...drafts.map(draft => ({ tool: 'save_home_panel', arguments: draft })),
  ], checkpoint: 'Saved twelve typed drafts for your exact review on Home.' });
  const executor = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  const browser = await chromium.launch({ headless: true });
  let page;
  t.after(async () => { if (!t.passed && page) {
    await page.screenshot({ path: join(app.data, 'general-panels-failure.png') }).catch(() => {});
    await writeFile(join(app.data, 'general-panels-failure.html'), await page.content()).catch(() => {});
  } await browser.close(); });
  async function pageFor(cookie) {
    const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
    const [name, value] = cookie.split('='); await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
    return context.newPage();
  }
  page = await pageFor(member); const ownerPage = await pageFor(app.cookie), errors = [];
  page.on('pageerror', error => errors.push(error.message));
  const home = `${app.base}/w/${workspace}/?view=home`;
  const card = index => page.locator(`[data-panel="${drafts[index].id}"]`);
  await page.goto(home); await page.getByRole('button', { name: 'Ask for a panel', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Ask for a Home panel', exact: true });
  assert.equal(await dialog.getByLabel('Panel type', { exact: true }).locator('option').count(), 8);
  await dialog.getByLabel('Panel type', { exact: true }).selectOption('chart');
  assert.equal(await dialog.getByLabel('Chart style', { exact: true }).locator('option').count(), 5);
  await dialog.getByLabel('Chart style', { exact: true }).selectOption('stacked_bar');
  await page.screenshot({ path: join(app.data, 'general-type-picker.png') });
  assert.equal(app.provider.requests.length, 0); await page.keyboard.press('Escape');
  const job = await app.send(command, chat, member); await app.done(job);
  let saved = (await app.request(`${app.path}/home-panels`, member)).value;
  assert.equal(saved.panels.length, 12);
  assert.ok(saved.panels.every(panel => !panel.active && panel.draft.available && panel.draft.source_operations === 1));
  await page.goto(home); await card(0).getByRole('button', { name: 'Review draft 1', exact: true }).click();
  await page.getByRole('dialog', { name: 'Review Home panel', exact: true }).getByRole('button', { name: 'Keep on Home', exact: true }).click();
  for (const draft of drafts.slice(1)) await app.request(`${app.path}/home-panels/${draft.id}/review`, member, { request: randomUUID(), revision: 1 });
  await page.reload(); await card(0).locator('.panel-comparison').waitFor();
  assert.equal(await card(0).locator('.panel-comparison').getAttribute('data-sentiment'), 'favorable');
  await card(2).getByRole('button', { name: 'Value', exact: true }).click();
  assert.deepEqual(await card(2).locator('tbody tr td:nth-child(2)').allTextContents(), ['2', '10', '100']);
  assert.equal(await card(3).locator('img').count(), 0);
  assert.equal(await card(4).getByRole('progressbar', { name: 'Measure B' }).getAttribute('aria-valuenow'), '100');
  assert.equal(await card(5).locator('.panel-timeline strong').first().textContent(), 'Later');
  for (const [i, variant] of ['line', 'bar', 'area', 'stacked_bar', 'donut'].entries()) {
    const chart = card(i+7); await chart.scrollIntoViewIfNeeded();
    assert.equal(await chart.getByRole('img', { name: `Chart ${variant} — ${variant.replace('_', ' ')}`, exact: true }).count(), 1);
    assert.doesNotMatch(await chart.locator('svg').evaluate(n => n.outerHTML), /NaN|Infinity/);
    await chart.screenshot({ path: join(app.data, `general-chart-${variant}.png`) });
  }
  await card(7).getByText('Chart values', { exact: true }).click();
  assert.equal(await card(7).locator('details tbody tr').count(), 3);
  await card(7).getByText('Chart values', { exact: true }).click();
  const before = app.provider.requests.length;
  await page.reload(); await card(0).waitFor(); assert.equal(app.provider.requests.length, before);
  await ownerPage.goto(home); await ownerPage.getByRole('heading', { name: 'Your workspace panels', exact: true }).waitFor();
  assert.equal(await ownerPage.locator('.workspace-panels [data-panel]').count(), 0);
  for (const [width, height, dark] of [[1440, 1000, false], [390, 844, false], [320, 640, true]]) {
    await page.setViewportSize({ width, height });
    await page.evaluate(dark => { document.documentElement.dataset.theme = dark ? 'dark' : 'light'; }, dark);
    for (const index of [0, 1, 2, 3, 4, 5, 6, 7, 11]) {
      await card(index).scrollIntoViewIfNeeded();
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
      if (index === 7 || index === 11) await card(index).screenshot({
        path: join(app.data, `general-panel-${index+1}-${width}-${dark ? 'dark' : 'light'}.png`),
      });
    }
    for (const [index, selector] of [[0, '.panel-comparison'], [7, 'svg'], [11, 'svg'], [7, '.panel-chart-legend'], [11, '.panel-chart-legend']]) {
      const bounds = await card(index).evaluate((node, selector) => {
        const viewport = node.querySelector('.panel-snapshot').getBoundingClientRect();
        const visual = node.querySelector(selector).getBoundingClientRect();
        return { top: visual.top-viewport.top, bottom: visual.bottom-viewport.top, height: viewport.height };
      }, selector);
      assert.ok(bounds.top >= -1 && bounds.bottom <= bounds.height+1,
        `The complete ${selector} in panel ${index+1} must fit its reading area at ${width}px: ${JSON.stringify(bounds)}`);
    }
    await card(0).scrollIntoViewIfNeeded();
    await page.screenshot({ path: join(app.data, `general-panels-${width}-${dark ? 'dark' : 'light'}.png`), fullPage: width === 1440 });
  }
  await app.stop(executor); await app.stopApi(); await app.startApi();
  await page.reload(); await card(0).getByText('12,480', { exact: true }).waitFor();
  assert.equal(app.provider.requests.length, before);
  await app.request(`${control}/rules`, app.cookie, { connection, operation: 'order.read', mode: 'block', revision: 1, chat });
  await card(3).getByRole('heading', { name: 'Panel source unavailable', exact: true }).waitFor();
  await until(async () => !(await page.content()).includes('INERT_ITEM'), 'Revoked source retained collection content', 12000);
  assert.ok(remote.calls.every(call => call.name === 'read_order')); assert.deepEqual(errors, []);
  const result = { passed: true, data: app.data, panels: 12, canonicalFamilies: 7, chartVariants: 5,
    nativeDraftReview: true, numericSorting: true, privateOwnerDenial: true, sourceRevocation: true,
    restart: true, noModelOnRead: true, externalWrites: 0, viewports: [1440, 390, 320] };
  await writeFile(join(app.data, 'general-home-panels-result.json'), JSON.stringify(result, null, 2));
  await writeFile(join(root, '.tmp/general-home-panels/live-result.json'), JSON.stringify(result, null, 2));
  app.pass('Seven general panel families, five chart variants, exact review, inert content, numeric sorting, responsive themes, restart and source denial');
});
