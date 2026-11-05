// Library for agents (LB1) with real Goose: an admin files policies in a folder and
// shares it with an agent for all staff; a colleague's question is answered from it with
// a citation Cuckoo adds, while an HR folder for "people who can see it" stays out of
// reach. The model is scripted; Goose, the controller and ayati_control are real.
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
const doc = text => ({ type: 'doc', content: [{ type: 'paragraph', content: [{ type: 'text', text }] }] });

test('A folder shared with an agent for all staff answers a colleague, with real Goose', { skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 180000 }, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true, runSeconds: 90, leaseSeconds: 15 });
  const w = app.path.split('/').at(-1);
  const member = await app.login(1);
  const who = async cookie => (await app.request('/api/identity/session', cookie)).value.user;
  const colleague = await who(member);
  const agent = { id: randomUUID(), name: 'Iris', role: 'Answers staff questions', model: 'fixture', audience: 'everyone', revision: 0, settings: { provider: 'openai', instructions: 'Answer from company documents.' } };
  await app.request(`${app.path}/agent-profiles`, app.cookie, agent);

  const browser = await chromium.launch({ headless: true }); const pages = []; const errors = [];
  t.after(async () => { if (!t.passed) for (const [i, page] of pages.entries()) { await page.screenshot({ path: join(app.data, `failure-${i}.png`) }).catch(() => {}); await writeFile(join(app.data, `failure-${i}.html`), await page.content()).catch(() => {}); } await browser.close(); });
  async function pageFor(cookie) { const context = await browser.newContext({ viewport: { width: 1280, height: 900 } }); const [name, value] = cookie.split('='); await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]); const page = await context.newPage(); page.on('pageerror', e => errors.push(e.message)); pages.push(page); return page; }
  const ownerPage = await pageFor(app.cookie);
  await ownerPage.goto(`${app.base}/w/${w}/?view=library`);
  await ownerPage.getByRole('button', { name: '＋ New folder', exact: true }).click();
  const naming = ownerPage.getByRole('dialog', { name: 'New folder' });
  await naming.getByLabel('Folder name').fill('Policies');
  await naming.getByRole('button', { name: 'Create folder', exact: true }).click();
  await ownerPage.waitForURL(/folder=/);
  const policies = new URL(ownerPage.url()).searchParams.get('folder');
  const returns = randomUUID();
  await app.request(`${app.path}/library`, app.cookie, { id: returns, request: randomUUID(), revision: 0, title: 'Returns policy', document: doc('The refund window is 30 days from delivery.'), folder: policies });
  await ownerPage.reload();
  await ownerPage.locator('.library-folder-header h2', { hasText: 'Policies' }).waitFor();
  await ownerPage.getByRole('link', { name: 'Returns policy', exact: true }).waitFor();
  await ownerPage.getByRole('button', { name: 'Share folder', exact: true }).click();
  const sharing = ownerPage.getByRole('dialog', { name: 'People and agents' });
  await sharing.getByText('Everything in this folder follows these settings.', { exact: false }).waitFor();
  await sharing.getByRole('checkbox', { name: 'Iris Read' }).check();
  await sharing.getByLabel('Use for, Iris').selectOption('staff');
  await ownerPage.screenshot({ path: join(app.data, 'library-folder-share.png') });
  await sharing.getByRole('button', { name: 'Save changes', exact: true }).click();
  await sharing.waitFor({ state: 'hidden' });
  await ownerPage.getByText('Only you · 1 agent (all staff). Pages and files here follow the folder\'s sharing.', { exact: true }).waitFor();
  await ownerPage.screenshot({ path: join(app.data, 'library-folder-desktop.png') });
  await ownerPage.setViewportSize({ width: 390, height: 844 });
  await ownerPage.screenshot({ path: join(app.data, 'library-folder-mobile.png') });
  assert.equal(await ownerPage.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
  await ownerPage.setViewportSize({ width: 1280, height: 900 });
  await ownerPage.goto(`${app.base}/w/${w}/?view=library`);
  await ownerPage.getByText('Folder · 1 item', { exact: true }).waitFor();
  await ownerPage.evaluate(() => { document.documentElement.dataset.theme = 'dark'; });
  await ownerPage.screenshot({ path: join(app.data, 'library-folders-dark.png') });
  // An HR folder the agent may use only for people who can see it.
  const hr = randomUUID(), salaries = randomUUID();
  await app.request(`${app.path}/library-folders`, app.cookie, { id: hr, request: randomUUID(), title: 'HR' });
  await app.request(`${app.path}/library`, app.cookie, { id: salaries, request: randomUUID(), revision: 0, title: 'Salaries', document: doc('Salary band for support is 40k.'), folder: hr });
  await app.request(`${app.path}/library/${hr}/share`, app.cookie, { request: randomUUID(), revision: 1, visibility: 'private', people: [], agents: [{ agent: agent.id, level: 'read', use_for: 'viewers' }] });
  // The colleague can't open either folder.
  await app.request(`${app.path}/library/${returns}`, member, undefined, 404);
  app.pass('An admin creates a folder, files a policy in it and shares it with Iris for all staff; desktop, 390 px and dark');

  await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  const chat = (await app.request(`${app.path}/chats`, member, { id: randomUUID(), name: 'Questions', kind: 'group', users: [], agents: [agent.id] })).value.id;
  const ask = 'What is our refund window?';
  app.provider.setControl(ask, { steps: [
    { tool: 'search_library', arguments: { query: 'refund window' } },
    { tool: 'read_document', arguments: { id: returns } },
    { tool: 'read_document', arguments: { id: salaries } },
  ], checkpoint: 'The refund window is 30 days from delivery.' });
  const job = await app.send(ask, chat, member, agent.id);
  const answer = await until(async () => (await app.history(chat, member)).find(m => m.id === job.reply && m.state === 'complete'), 'Agent did not finish', 45000);
  assert.ok(answer.content.startsWith('The refund window is 30 days from delivery.'));
  assert.match(answer.content, /Library documents read for this answer:/);
  assert.match(answer.content, new RegExp(`\\[Returns policy · revision 1\\]\\(/w/${w}/\\?view=library&item=${returns}&source_revision=1\\)`));
  assert.doesNotMatch(answer.content, /Salaries/);
  assert.equal(app.sql(`SELECT string_agg(item, ',') FROM relay_library_reads WHERE reply='${job.reply}'`), returns, 'only the staff policy was read');
  const memberPage = await pageFor(member);
  await memberPage.goto(`${app.base}/w/${w}/?view=chat&conversation=${chat}`);
  await memberPage.getByRole('link', { name: 'Returns policy · revision 1', exact: true }).waitFor();
  await memberPage.screenshot({ path: join(app.data, 'library-cited-answer.png') });
  app.pass('Real Goose: a colleague without access gets the staff policy answer with a citation Cuckoo adds; the HR folder is never read');

  // In a conversation with the colleague, even the owner's question can't use HR.
  const shared = (await app.request(`${app.path}/chats`, app.cookie, { id: randomUUID(), name: 'Team', kind: 'group', users: [colleague.id], agents: [agent.id] })).value.id;
  const salaryAsk = 'What is the support salary band?';
  app.provider.setControl(salaryAsk, { steps: [{ tool: 'read_document', arguments: { id: salaries } }], checkpoint: 'I can\'t share that here.' });
  const group = await app.send(salaryAsk, shared, app.cookie, agent.id);
  await until(async () => (await app.history(shared, app.cookie)).find(m => m.id === group.reply && m.state === 'complete'), 'Agent did not finish', 45000);
  assert.equal(app.sql(`SELECT count(*) FROM relay_library_reads WHERE reply='${group.reply}'`), '0');
  app.pass('Real Goose: a document for people who can see it is not used where a colleague without access will read the answer');
  assert.deepEqual(errors, []);
});
