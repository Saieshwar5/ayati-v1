// App events (Apps AP10) with real Goose: the builder of Leave asks Nia to act when a
// request is added; a colleague asks for leave in the app; Nia, working for the
// builder in their conversation, tells them and opens a task to decide. The model is
// scripted; Goose, the controller, the dispatcher and ayati_control are real.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { until } from './fixtures/polling.mjs';
const root = resolve(import.meta.dirname, '..');
const { chromium } = createRequire(join(root, 'web/package.json'))('playwright');

test('A leave request added in the app starts work for the builder\'s agent, with real Goose', { skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 180000 }, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true, runSeconds: 90, leaseSeconds: 15 });
  const w = app.path.split('/').at(-1);
  const [ravi, mina] = [await app.login(1), await app.login(2)];
  const minaUser = (await app.request('/api/identity/session', mina)).value.user;
  const agent = { id: randomUUID(), name: 'Nia', role: 'Helps with HR', model: 'fixture', audience: 'everyone', revision: 0, settings: { provider: 'openai', instructions: 'Help with leave.' } };
  await app.request(`${app.path}/agent-profiles`, app.cookie, agent);
  await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  const id = randomUUID();
  await app.request(`${app.path}/apps/built`, ravi, { id, name: 'Leave', description: '', audience: 'everyone', people: [], template: 'leave' });
  await app.request(`${app.path}/apps/built/${id}/publish`, ravi, { note: 'First', revision: 1 });
  const browser = await chromium.launch({ headless: true }); const pages = []; const errors = [];
  t.after(async () => { if (!t.passed) for (const [i, page] of pages.entries()) { await page.screenshot({ path: join(app.data, `failure-${i}.png`) }).catch(() => {}); await writeFile(join(app.data, `failure-${i}.html`), await page.content()).catch(() => {}); } await browser.close(); });
  async function pageFor(cookie) { const context = await browser.newContext({ viewport: { width: 1280, height: 900 } }); const [name, value] = cookie.split('='); await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]); const page = await context.newPage(); page.on('pageerror', e => errors.push(e.message)); pages.push(page); return page; }

  const raviPage = await pageFor(ravi);
  await raviPage.goto(`${app.base}/w/${w}/?view=apps`);
  const section = raviPage.getByRole('region', { name: 'When something happens' });
  await section.getByRole('button', { name: 'Ask an agent: new requests', exact: true }).click();
  const dialog = raviPage.getByRole('dialog', { name: 'When new requests are added' });
  await dialog.getByLabel('Agent').selectOption(agent.id);
  await dialog.getByLabel('What should it do?').fill('Tell me who asked and for how long, and open a task for me to decide.');
  await dialog.getByRole('button', { name: 'Save', exact: true }).click();
  await section.getByText(/Nia, for .*: Tell me who asked/).waitFor();
  await raviPage.screenshot({ path: join(app.data, 'built-app-events.png') });
  const watcher = (await app.request(`${app.path}/apps/built/${id}/watchers`, ravi)).value.watchers[0];
  app.pass('The builder asks Nia, from the app\'s panel, to act when a leave request is added');

  // The record's fields reach the agent in a fixed order (JSON object keys by length).
  const first = `New in the app "Leave" (requests), added by ${minaUser.display_name}: days: 2, start: 2026-11-10, reason: Family visit, status: waiting`;
  app.provider.setControl(first, { steps: [
    { tool: 'create_task', arguments: { title: `Decide ${minaUser.display_name}'s leave`, done_when: 'The request is approved or declined in Leave' } },
  ], checkpoint: `${minaUser.display_name} asked for 2 days from 10 November. I opened a task for you to decide.` });
  const minaPage = await pageFor(mina);
  await minaPage.goto(`${app.base}/w/${w}/?view=apps&run=${id}`);
  const frame = minaPage.frameLocator('iframe[title="Leave"]');
  await frame.getByLabel('First day').fill('2026-11-10');
  await frame.getByLabel('Days').fill('2');
  await frame.getByLabel('Reason').fill('Family visit');
  await frame.getByRole('button', { name: 'Ask for leave' }).click();
  await frame.getByText('Saved.', { exact: true }).waitFor();
  const reply = await until(async () => (await app.history(watcher.chat, ravi)).find(m => m.state === 'complete' && m.content?.includes('I opened a task for you to decide')), 'Nia did not act on the request', 60000);
  assert.ok(reply);
  assert.equal(app.sql(`SELECT count(*) FROM relay_tasks WHERE title='Decide ${minaUser.display_name.replaceAll("'", "''")}''s leave'`), '1');
  assert.equal(app.sql(`SELECT state FROM relay_built_app_events WHERE app='${id}'`), 'started');
  await raviPage.goto(`${app.base}/w/${w}/?view=chat&conversation=${watcher.chat}`);
  await raviPage.getByText('I opened a task for you to decide', { exact: false }).first().waitFor();
  await raviPage.screenshot({ path: join(app.data, 'built-app-event-reply.png') });
  const audit = (await app.request(`${app.path}/activity?kind=all`, app.cookie)).value.activity.map(e => e.sentence).join('\n');
  assert.match(audit, /Asked Nia to act on new requests in "Leave"/);
  assert.deepEqual(errors, []);
  app.pass('Real Goose: the colleague\'s request becomes Nia\'s work for the builder; Nia tells them and opens a task to decide');
});
