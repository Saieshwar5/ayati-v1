// "Ask first" for app actions (Apps AP10) with real Goose: filing leave in Leave is set to
// ask first; asked in Chat, Nia is handed Cuckoo's own question, puts it to Mina, and
// files the leave only after Mina answers yes in the browser. The model is scripted;
// Goose, the controller, ayati_control and the question card are real.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { randomUUID, createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { until } from './fixtures/polling.mjs';
const root = resolve(import.meta.dirname, '..');
const { chromium } = createRequire(join(root, 'web/package.json'))('playwright');

test('An ask-first app action runs only after the person says yes, with real Goose', { skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 180000 }, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true, runSeconds: 90, leaseSeconds: 15 });
  const w = app.path.split('/').at(-1);
  const mina = await app.login(2);
  const agent = { id: randomUUID(), name: 'Nia', role: 'Helps with HR', model: 'fixture', audience: 'everyone', revision: 0, settings: { provider: 'openai', instructions: 'Help with leave.' } };
  await app.request(`${app.path}/agent-profiles`, app.cookie, agent);
  const id = randomUUID();
  await app.request(`${app.path}/apps/built`, app.cookie, { id, name: 'Leave', description: '', audience: 'everyone', people: [], template: 'leave' });
  await app.request(`${app.path}/apps/built/${id}/publish`, app.cookie, { note: 'First', revision: 1 });
  await app.request(`${app.path}/apps/built/${id}/agent-rules`, app.cookie, { action: 'file_leave', mode: 'ask' });
  await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  const browser = await chromium.launch({ headless: true }); const pages = []; const errors = [];
  t.after(async () => { if (!t.passed) for (const [i, page] of pages.entries()) { await page.screenshot({ path: join(app.data, `failure-${i}.png`) }).catch(() => {}); await writeFile(join(app.data, `failure-${i}.html`), await page.content()).catch(() => {}); } await browser.close(); });
  async function pageFor(cookie) { const context = await browser.newContext({ viewport: { width: 1280, height: 900 } }); const [name, value] = cookie.split('='); await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]); const page = await context.newPage(); page.on('pageerror', e => errors.push(e.message)); pages.push(page); return page; }

  // Cuckoo's question for these exact fields: its key and wording are fixed by them.
  const run = { app: id, action: 'file_leave', record: null, data: { start: '2026-11-10', days: 2 }, revision: null };
  const key = `app-${createHash('sha256').update(JSON.stringify(run)).digest('base64url').slice(0, 20)}`;
  const specification = { version: 1, kind: 'single_choice', title: 'May Nia file a leave request in Leave?', description: 'start: 2026-11-10 · days: 2', fallback: 'Reply yes or no.', options: [{ id: 'yes', label: 'Yes, go ahead' }, { id: 'no', label: 'No' }] };
  const ask = 'File two days of leave for me from 10 November';
  const action = { tool: 'run_app_action', arguments: { app: id, action: 'file_leave', data: { start: '2026-11-10', days: 2 } } };
  app.provider.setControl(ask, {
    steps: [action, { tool: 'ask_question', arguments: { key, specification } }],
    checkpoint: 'Filing leave asks you first; I sent you the question.',
    afterQuestion: [action],
  });
  const chat = (await app.request(`${app.path}/chats`, mina, { id: randomUUID(), name: 'Leave', kind: 'group', users: [], agents: [agent.id] })).value.id;
  const job = await app.send(ask, chat, mina, agent.id);
  const question = await until(async () => { const row = app.sql(`SELECT row_to_json(q) FROM relay_task_questions q WHERE reply='${job.reply}'`); return row ? JSON.parse(row) : null; }, 'Nia did not ask', 45000);
  assert.equal(question.request_key, key);
  assert.equal(app.sql(`SELECT count(*) FROM relay_built_app_records WHERE app='${id}'`), '0', 'nothing filed before the answer');
  app.pass('Real Goose: asked to file leave, Nia is handed Cuckoo\'s question and puts it to Mina; nothing is filed yet');

  const page = await pageFor(mina);
  await page.goto(`${app.base}/w/${w}/?view=chat&conversation=${chat}&message=${job.reply}`);
  await page.getByRole('button', { name: 'Answer question', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: specification.title, exact: true });
  await dialog.getByText('start: 2026-11-10 · days: 2').waitFor();
  await page.screenshot({ path: join(app.data, 'built-app-ask-first.png') });
  await dialog.getByRole('radio', { name: 'Yes, go ahead', exact: true }).check();
  await dialog.getByRole('button', { name: 'Submit answer', exact: true }).click();
  const done = await until(async () => (await app.history(chat, mina)).find(m => m.id === job.reply && m.state === 'complete'), 'The request did not finish after the answer', 45000);
  const continued = JSON.parse(done.content.replace(/^CONTINUED_RESULT /, ''));
  assert.equal(continued.questions[0].values.choices[0], 'yes');
  assert.equal(JSON.parse(continued.results[0]).state, 'done');
  assert.equal(app.sql(`SELECT count(*) FROM relay_built_app_records WHERE app='${id}' AND stage='live'`), '1');
  const audit = (await app.request(`${app.path}/activity?kind=all`, app.cookie)).value.activity.map(e => e.sentence).join('\n');
  assert.match(audit, /Agents may "File a leave request" in "Leave" after asking the person/);
  assert.match(audit, /Nia did "File a leave request" in "Leave" for/);
  assert.deepEqual(errors, []);
  app.pass('Mina answers yes in the browser; the same request then files her leave; the rule and the run are audited');
});
