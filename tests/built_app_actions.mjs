// Agents acting in built apps (Apps AP10) with real Goose: a member starts Leave
// requests from its template, publishes it and allows agents to file leave; a colleague
// asks the agent in Chat, which files the request as her; approving it stays blocked.
// The model is scripted; Goose, the controller and ayati_control are real.
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

test('An agent files leave from Chat as the person, and cannot approve it, with real Goose', { skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 180000 }, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true, runSeconds: 90, leaseSeconds: 15 });
  const w = app.path.split('/').at(-1);
  const [ravi, mina] = [await app.login(1), await app.login(2)];
  const minaUser = (await app.request('/api/identity/session', mina)).value.user;
  const agent = { id: randomUUID(), name: 'Nia', role: 'Helps with HR', model: 'fixture', audience: 'everyone', revision: 0, settings: { provider: 'openai', instructions: 'Help people with leave.' } };
  await app.request(`${app.path}/agent-profiles`, app.cookie, agent);
  await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  const browser = await chromium.launch({ headless: true }); const pages = []; const errors = [];
  t.after(async () => { if (!t.passed) for (const [i, page] of pages.entries()) { await page.screenshot({ path: join(app.data, `failure-${i}.png`) }).catch(() => {}); await writeFile(join(app.data, `failure-${i}.html`), await page.content()).catch(() => {}); } await browser.close(); });
  async function pageFor(cookie) { const context = await browser.newContext({ viewport: { width: 1280, height: 900 } }); const [name, value] = cookie.split('='); await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]); const page = await context.newPage(); page.on('pageerror', e => errors.push(e.message)); pages.push(page); return page; }

  const raviPage = await pageFor(ravi);
  await raviPage.goto(`${app.base}/w/${w}/?view=apps`);
  await raviPage.getByRole('button', { name: 'Build an app', exact: true }).click();
  const form = raviPage.getByRole('dialog', { name: 'Build an app' });
  await form.getByLabel('Start from').selectOption('leave');
  await form.getByLabel('Name', { exact: true }).fill('Leave');
  await form.getByRole('button', { name: 'Start building', exact: true }).click();
  await form.waitFor({ state: 'hidden' });
  await raviPage.getByRole('button', { name: 'Code and publish', exact: true }).click();
  const drawer = raviPage.getByRole('dialog', { name: 'Leave: code' });
  await drawer.getByText(/Every check passes/).waitFor();
  await drawer.getByRole('button', { name: 'Publish', exact: true }).click();
  await drawer.getByText('Published version 1. People you chose can open it now.', { exact: true }).waitFor();
  await drawer.getByRole('button', { name: 'Close' }).click();
  const rules = raviPage.getByRole('region', { name: 'Agents can' });
  await rules.getByLabel('Agents: File a leave request').waitFor();
  assert.equal(await rules.getByLabel('Agents: Approve or decline a leave request').inputValue(), 'block');
  await rules.getByLabel('Agents: File a leave request').selectOption('allow');
  await until(async () => (await app.request(`${app.path}/apps/built/${(await app.request(`${app.path}/apps`, ravi)).value.built[0].id}/agent-rules`, ravi)).value.actions[0].mode === 'allow', 'The rule was not saved', 10000);
  await raviPage.screenshot({ path: join(app.data, 'built-app-agent-rules.png') });
  const id = (await app.request(`${app.path}/apps`, ravi)).value.built[0].id;
  app.pass('A member starts Leave from its template, publishes it as it is, and allows agents only to file leave');

  const chat = (await app.request(`${app.path}/chats`, mina, { id: randomUUID(), name: 'Leave', kind: 'group', users: [], agents: [agent.id] })).value.id;
  const ask = 'File two days of leave from 10 November, then approve it';
  app.provider.setControl(ask, { steps: [
    { tool: 'app_actions', arguments: {} },
    { tool: 'run_app_action', arguments: { app: id, action: 'file_leave', data: { start: '2026-11-10', days: 2, reason: 'Family visit' } } },
    { tool: 'run_app_action', arguments: { app: id, action: 'decide_leave', record: 'unknown', data: { status: 'approved' }, revision: 1 } },
  ] });
  const before = app.provider.requests.length;
  const job = await app.send(ask, chat, mina, agent.id);
  const reply = await until(async () => (await app.history(chat, mina)).find(m => m.id === job.reply && m.state === 'complete'), 'Nia did not finish', 60000);
  const [offered, filed, approved] = JSON.parse(reply.content.replace(/^CONTROL_RESULT /, '')).map(r => JSON.parse(r));
  assert.deepEqual(offered.apps[0].actions.map(a => a.id), ['file_leave']);
  assert.equal(filed.state, 'done');
  assert.equal(filed.result.by, minaUser.display_name);
  assert.equal(filed.result.agent, 'Nia');
  assert.equal(approved.state, 'refused');
  const prompts = app.provider.requests.slice(before).map(r => JSON.stringify(r));
  assert.ok(prompts.some(p => p.includes('You can act in apps built in Cuckoo')), 'the instructions mention app actions');
  app.pass('Real Goose: Nia sees only "file leave", files it as Mina (recorded with Nia), and approving is refused');

  const minaPage = await pageFor(mina);
  await minaPage.goto(`${app.base}/w/${w}/?view=apps&run=${id}`);
  const frame = minaPage.frameLocator('iframe[title="Leave"]');
  await frame.getByText(`${minaUser.display_name} · 2026-11-10 · 2 · Family visit · waiting`).waitFor();
  await minaPage.screenshot({ path: join(app.data, 'built-app-agent-filed.png') });
  const audit = (await app.request(`${app.path}/activity?kind=all`, app.cookie)).value.activity.map(e => e.sentence).join('\n');
  assert.match(audit, /Nia did "File a leave request" in "Leave" for/);
  assert.match(audit, /Agents may "File a leave request" in "Leave"/);
  assert.deepEqual(errors, []);
  app.pass('Mina opens Leave and sees her request waiting; the agent\'s action and the rule are audited');
});
