// Add more apps (schema 135), in a real browser against a real controller: an
// admin searches the library, reviews HubSpot's actions and adds it; the app is
// connected on an agent through the provider's sign-in page; changing its
// actions ends that connection, and removing it stops offering it. The library
// here is a local stand-in for Composio's.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { composioProvider, COMPOSIO_KEY } from './fixtures/composio_provider.mjs';
const root = resolve(import.meta.dirname, '..');
const { chromium } = createRequire(join(root, 'web/package.json'))('playwright');

test('an admin adds HubSpot from the library, it is connected on an agent, then changed and removed', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 180000,
}, async t => {
  const composio = await composioProvider(); t.after(() => composio.close());
  const directory = await mkdtemp(join(root, '.tmp/added-apps-'));
  const key = join(directory, 'composio-key'); await writeFile(key, COMPOSIO_KEY, { mode: 0o600 });
  const app = await workerFixture(t, { mode: 'pooled', control: true,
    apiEnv: { AYATI_COMPOSIO_KEY_FILE: key, AYATI_COMPOSIO_BASE_URL: composio.url } });
  const w = app.path.split('/').at(-1);
  const browser = await chromium.launch({ headless: true }); const errors = [];
  const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  const [name, value] = app.cookie.split('=');
  await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
  const page = await context.newPage(); page.on('pageerror', e => errors.push(e.message));
  page.on('dialog', dialog => dialog.accept());
  t.after(async () => { if (!t.passed) await page.screenshot({ path: join(app.data, 'failure.png') }).catch(() => {}); await browser.close(); });
  const shot = file => page.screenshot({ path: join(app.data, file) });
  const allowed = async () => (await app.request(`${app.path}/apps/allowed`, app.cookie)).value;

  // Settings, Connections: search the library and review HubSpot's actions.
  await page.goto(`${app.base}/w/${w}/?view=settings&section=connections`);
  await page.getByRole('button', { name: 'Add more apps' }).click();
  const search = page.getByRole('dialog', { name: 'Add more apps' });
  await search.getByRole('searchbox').fill('hub');
  await search.getByRole('button', { name: 'Search' }).click();
  await search.getByText('Needs an API key or its own sign-in setup').waitFor();
  await shot('1-search.png');
  await search.getByRole('button', { name: 'Choose actions for HubSpot' }).click();
  const review = page.getByRole('dialog', { name: 'Add HubSpot' });
  await review.getByRole('checkbox', { name: /Create note/ }).waitFor();
  assert.equal(await review.getByRole('checkbox', { name: /Create contacts in bulk/ }).isDisabled(), true);
  await review.getByRole('checkbox', { name: /Create note/ }).check();
  await review.getByRole('button', { name: 'Show more actions' }).click();
  await review.getByRole('checkbox', { name: /Search contacts/ }).check();
  assert.equal(await review.getByRole('combobox', { name: 'What Search contacts does' }).inputValue(), 'read');
  assert.equal(await review.getByRole('combobox', { name: 'What Create note does' }).inputValue(), 'write');
  await review.getByText('2 of 20 chosen').waitFor();
  await shot('2-review.png');
  await review.getByRole('button', { name: 'Add HubSpot' }).click();
  await page.getByText('HubSpot was added and is allowed.').waitFor();
  await shot('3-added.png');
  let hubspot = (await allowed()).apps.find(a => a.id === 'hubspot');
  assert.equal(hubspot.allowance, 'allowed');
  assert.deepEqual(hubspot.added.actions.map(a => [a.tool, a.effect]), [['HUBSPOT_CREATE_NOTE', 'write'], ['HUBSPOT_SEARCH_CONTACTS', 'read']]);
  app.pass('An admin searched the library, reviewed HubSpot and added it; it is allowed with its two actions');

  // An agent's Connections: HubSpot is offered like a built-in app and connects through its sign-in page.
  const agent = { id: randomUUID(), name: 'Sales Desk', role: 'Sales', model: 'fixture', audience: 'everyone', revision: 0, settings: { provider: 'openai', instructions: 'Keep customer notes.' } };
  await app.request(`${app.path}/agent-profiles`, app.cookie, agent);
  await page.goto(`${app.base}/w/${w}/?view=agents&selected=${agent.id}&section=connections`);
  await page.getByRole('button', { name: 'Add app' }).first().click();
  await page.getByRole('button', { name: 'Connect HubSpot' }).waitFor();
  await shot('4-add-app.png');
  await page.getByRole('button', { name: 'Connect HubSpot' }).click();
  await page.getByRole('button', { name: /^Continue to HubSpot/ }).click();
  await page.waitForURL(/connected=1/);
  const connections = async () => (await app.request(`${app.path}/agent-profiles/${agent.id}/control`, app.cookie)).value.connections;
  let connection = (await connections()).find(c => c.server === 'hubspot');
  assert.deepEqual(connection.operations.map(o => [o.id, o.effect, o.mode]), [['hubspot.create_note', 'write', 'ask'], ['hubspot.search_contacts', 'read', 'allow']]);
  await shot('5-connected.png');
  app.pass('HubSpot connected on an agent through the sign-in page; its read is allowed and its change asks first');

  // Changing the actions warns, ends the connection and keeps the app allowed.
  await page.goto(`${app.base}/w/${w}/?view=settings&section=connections`);
  await page.getByRole('button', { name: 'Change the actions of HubSpot' }).click();
  const change = page.getByRole('dialog', { name: 'Change HubSpot' });
  await change.getByText('Saving ends 1 connection of HubSpot.').waitFor();
  await change.getByRole('checkbox', { name: /Create note/ }).uncheck();
  await shot('6-change.png');
  await change.getByRole('button', { name: 'Save changes' }).click();
  await page.getByText('HubSpot was changed. 1 connection ended; their owners connect again.').waitFor();
  assert.equal((await connections()).some(c => c.server === 'hubspot'), false);
  assert.equal(composio.removed.length, 1, 'the ended account was revoked at the provider');
  hubspot = (await allowed()).apps.find(a => a.id === 'hubspot');
  assert.deepEqual(hubspot.added.actions.map(a => a.tool), ['HUBSPOT_SEARCH_CONTACTS']);
  app.pass('Changing the actions ended the connection, revoked its account and kept the app allowed');

  // Removing it stops offering it.
  await page.getByRole('button', { name: 'Remove HubSpot' }).click();
  await page.getByText('HubSpot was removed.').waitFor();
  assert.equal((await allowed()).apps.some(a => a.id === 'hubspot'), false);
  const catalog = (await app.request(`${app.path}/apps/catalog?agent=${agent.id}`, app.cookie)).value;
  assert.equal(catalog.apps.some(a => a.id === 'hubspot'), false);
  assert.equal(app.processes().some(p => p.logs.includes(COMPOSIO_KEY)), false);
  assert.deepEqual(errors, []);
  app.pass('Removing HubSpot stopped offering it; the provider key never reached the logs');
});
