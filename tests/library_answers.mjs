// Answers saved from Chat (Library LB5) with real Goose: the agent finds nothing about
// visitor parking; the owner answers in the conversation and saves the answer to the
// Policies folder from the message's menu; asked again, the agent finds the saved page
// and Cuckoo cites it. The model is scripted; Goose, the controller and ayati_control
// are real.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { writeFile, mkdtemp } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { until } from './fixtures/polling.mjs';
const root = resolve(import.meta.dirname, '..');
const { chromium } = createRequire(join(root, 'web/package.json'))('playwright');

test('An answer saved from Chat answers the agent next time, with real Goose', { skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 180000 }, async t => {
  const storage = await mkdtemp(join(root, '.tmp/library-files-'));
  const app = await workerFixture(t, { mode: 'pooled', control: true, runSeconds: 90, leaseSeconds: 15, apiEnv: { CUCKOO_LIBRARY_LOCAL_ROOT: storage } });
  const w = app.path.split('/').at(-1);
  const member = await app.login(1);
  const colleague = (await app.request('/api/identity/session', member)).value.user;
  const agent = { id: randomUUID(), name: 'Iris', role: 'Answers staff questions', model: 'fixture', audience: 'everyone', revision: 0, settings: { provider: 'openai', instructions: 'Answer from company documents.' } };
  await app.request(`${app.path}/agent-profiles`, app.cookie, agent);
  const policies = randomUUID();
  await app.request(`${app.path}/library-folders`, app.cookie, { id: policies, request: randomUUID(), title: 'Policies' });
  await app.request(`${app.path}/library/${policies}/share`, app.cookie, { request: randomUUID(), revision: 1, visibility: 'private', people: [], agents: [{ agent: agent.id, level: 'read', use_for: 'staff' }] });
  await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  const browser = await chromium.launch({ headless: true }); const pages = []; const errors = [];
  t.after(async () => { if (!t.passed) for (const [i, page] of pages.entries()) { await page.screenshot({ path: join(app.data, `failure-${i}.png`) }).catch(() => {}); await writeFile(join(app.data, `failure-${i}.html`), await page.content()).catch(() => {}); } await browser.close(); });
  async function pageFor(cookie) { const context = await browser.newContext({ viewport: { width: 1280, height: 900 } }); const [name, value] = cookie.split('='); await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]); const page = await context.newPage(); page.on('pageerror', e => errors.push(e.message)); pages.push(page); return page; }

  const chat = (await app.request(`${app.path}/chats`, app.cookie, { id: randomUUID(), name: 'Front desk', kind: 'group', users: [colleague.id], agents: [agent.id] })).value.id;
  const ask = 'Where do visitors park?';
  app.provider.setControl(ask, { steps: [{ tool: 'search_library', arguments: { query: 'visitors park' } }] });
  const first = await app.send(ask, chat, member, agent.id);
  const nothing = await until(async () => (await app.history(chat, member)).find(m => m.id === first.reply && m.state === 'complete'), 'Iris did not finish', 45000);
  assert.deepEqual(JSON.parse(JSON.parse(nothing.content.replace(/^CONTROL_RESULT /, ''))[0]).results, []);
  app.pass('Real Goose: asked where visitors park, Iris finds nothing in the Library');

  const reply = 'Visitors park in bay C, behind the warehouse.';
  await app.request(`${app.path}/chats/${chat}/messages`, app.cookie, { id: randomUUID(), content: reply });
  const page = await pageFor(app.cookie);
  await page.goto(`${app.base}/w/${w}/?view=chat&conversation=${chat}`);
  const message = page.locator('article.chat-message', { hasText: reply });
  await message.hover();
  await message.getByRole('button', { name: 'More actions' }).click();
  await message.getByRole('menuitem', { name: 'Save to Library' }).click();
  const dialog = page.getByRole('dialog', { name: 'Save to Library' });
  await dialog.getByLabel('Question or title').fill('Where do visitors park?');
  await dialog.getByLabel('Folder').selectOption(policies);
  await page.screenshot({ path: join(app.data, 'library-save-answer.png') });
  await dialog.getByRole('button', { name: 'Save', exact: true }).click();
  await dialog.getByText('Saved. Agents given this folder can answer it from now on.', { exact: true }).waitFor();
  const saved = new URL(await dialog.getByRole('link', { name: 'Open the page' }).getAttribute('href'), app.base).searchParams.get('item');
  const item = (await app.request(`${app.path}/library/${saved}`, app.cookie)).value;
  assert.equal(item.folder, policies);
  assert.equal(item.title, 'Where do visitors park?');
  app.pass('The owner saves their answer to Policies from the message\'s menu');

  app.provider.setControl('Where do visitors park, again?', { steps: [{ tool: 'search_library', arguments: { query: 'visitors park' } }, { tool: 'read_document', arguments: { id: saved } }], checkpoint: 'Visitors park in bay C, behind the warehouse.' });
  const second = await app.send('Where do visitors park, again?', chat, member, agent.id);
  const answered = await until(async () => (await app.history(chat, member)).find(m => m.id === second.reply && m.state === 'complete'), 'Iris did not finish', 45000);
  assert.ok(answered.content.startsWith('Visitors park in bay C'));
  assert.match(answered.content, /\[Where do visitors park\? · revision 1\]/);
  const audit = (await app.request(`${app.path}/activity?kind=all`, app.cookie)).value.activity.map(e => e.sentence).join('\n');
  assert.match(audit, /Saved an answer from Chat as "Where do visitors park\?"/);
  assert.deepEqual(errors, []);
  app.pass('Real Goose: asked again, Iris reads the saved answer and Cuckoo cites it; saving is audited');
});
