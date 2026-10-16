import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import { browserSite } from './fixtures/browser_site.mjs';
import { setup, until, root, vmManifest } from './fixtures/goose_app.mjs';
const { chromium } = createRequire(join(root, 'web/package.json'))('playwright');

test('memory can be enabled and disabled without data loss; unavailable browser is rejected', { timeout: 60000, skip: !!vmManifest }, async t => {
  const app = await setup(t, true, { AYATI_MEMORY: '0', AYATI_BROWSER: '0' });
  const browser = await chromium.launch(); t.after(() => browser.close());
  const page = await browser.newPage({ viewport: { width: 390, height: 844 } });
  const profile = async () => (await app.request('/agents')).agents[0].profile;
  const setTools = async (memory, browser) => {
    const p = await profile();
    return app.request('/agents/local-agent/profile', { name: p.name, role: p.role, expected_revision: p.revision, memory, browser });
  };
  await page.goto(app.base.replace('/api', '/?agent=local-agent'));
  await page.getByRole('button', { name: 'Details', exact: true }).click();
  await page.getByRole('button', { name: 'Edit agent settings' }).click();
  const dialog = page.getByRole('dialog', { name: 'Edit agent', exact: true });
  assert(await dialog.getByLabel('Browser', { exact: true }).isDisabled());
  await dialog.getByLabel('Sourced memory').check();
  await dialog.getByRole('button', { name: 'Save changes' }).click();
  await dialog.getByText('Saved.', { exact: false }).waitFor();
  assert.equal((await profile()).memory, true);
  await mkdir(join(root, '.test-runs/tools'), { recursive: true });
  await page.screenshot({ path: join(root, '.test-runs/tools/mobile.png') });
  await app.request('/memory', { key: 'report.style', content: 'Keep this durable note' });
  const task = await app.submit('memory ' + JSON.stringify({ tool: 'search', arguments: { query: 'report' } }));
  await until(async () => (await app.state()).permissions.length, 'Memory approval missing');
  await app.request(`/permissions/${(await app.state()).permissions[0].id}`, { allow: true });
  assert.equal((await app.done(task.id)).task.status, 'completed');
  assert(app.provider.requests.at(-1).tools.some(t => t.function.name.startsWith('ayati_memory__')));
  await setTools(false, false);
  assert.equal((await app.request('/memory')).enabled, false);
  assert.equal((await app.request('/memory')).memories[0].content, 'Keep this durable note');
  await app.done((await app.submit('With memory disabled')).id);
  assert(!app.provider.requests.at(-1).tools.some(t => t.function.name.startsWith('ayati_memory__')));
  const p = await profile();
  const bad = await fetch(app.base + '/agents/local-agent/profile', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name: p.name, role: p.role, expected_revision: p.revision, memory: true, browser: true }) });
  assert.equal(bad.status, 400);
  assert.equal((await profile()).revision, p.revision, 'Invalid browser must roll back the whole edit');
  assert.equal((await profile()).memory, false);
  await app.stop(); await app.start();
  assert.equal((await profile()).memory, false);
  await app.done((await app.submit('Still disabled after restart')).id);
  assert(!app.provider.requests.at(-1).tools.some(t => t.function.name.startsWith('ayati_memory__')));
  await setTools(true, false);
  assert.equal((await app.request('/memory')).memories[0].content, 'Keep this durable note');
  await app.done((await app.submit('Enabled again')).id);
  assert(app.provider.requests.at(-1).tools.some(t => t.function.name.startsWith('ayati_memory__')));
});


test('compatible VM can enable tools that were disabled at creation', { timeout: 120000, skip: !vmManifest }, async t => {
  const site = await browserSite(); t.after(site.close);
  const app = await setup(t, false, { AYATI_MEMORY: '0', AYATI_BROWSER: '0', AYATI_BROWSER_FIXTURE_PORT: String(site.port) });
  const profile = (await app.request('/agents')).agents[0].profile;
  assert.equal(profile.browser_available, true);
  await app.request('/agents/local-agent/profile', { name: profile.name, role: profile.role, expected_revision: 0, memory: true, browser: true });
  const task = await app.submit('browser ' + JSON.stringify({ tool: 'open', arguments: { url: 'http://fixture.ayati.invalid' } }));
  const outcome = await until(async () => {
    const state = await app.state();
    for (const permission of state.permissions) await app.request(`/permissions/${permission.id}`, { allow: true });
    return state.tasks.find(t => t.id === task.id && ['completed','failed'].includes(t.status));
  }, 'Newly enabled browser did not complete', 60000);
  assert.equal(outcome.status, 'completed', outcome.detail);
  const tools = app.provider.requests.at(-1).tools.map(t => t.function.name);
  assert(tools.some(t => t.startsWith('ayati_browser__')));
  assert(tools.some(t => t.startsWith('ayati_memory__')));
});
