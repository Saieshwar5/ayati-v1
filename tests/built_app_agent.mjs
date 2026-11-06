// An agent builds an app (Apps BA3) with real Goose: a member starts an app on the Apps
// page and asks an agent to build it; their chat opens with the request ready; the
// agent writes the draft through ayati_control; the member previews the draft, which
// keeps data through the bridge, and publishes it. The model is scripted; Goose, the
// controller and ayati_control are real.
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

const manifest = JSON.stringify({ name: 'Stock count', description: 'Count the shelves', roles: ['user'],
  collections: { counts: { fields: { item: 'text', quantity: 'number' }, access: { user: ['create', 'read'] } } } }, null, 2);
const html = '<!doctype html>\n<html lang="en">\n<head>\n<meta charset="utf-8">\n<title>Stock count</title>\n<link rel="stylesheet" href="style.css">\n</head>\n<body>\n<main>\n<h1>Stock count</h1>\n<p id="state">Loading…</p>\n<ul id="counts"></ul>\n</main>\n<script src="app.js"></script>\n</body>\n</html>\n';
const code = `const state = document.getElementById('state');
const list = document.getElementById('counts');
(async () => {
  await cuckoo.create('counts', { item: 'Blue mugs', quantity: 12 });
  const { records } = await cuckoo.list('counts');
  list.replaceChildren(...records.map(r => Object.assign(document.createElement('li'), { textContent: r.data.item + ': ' + r.data.quantity })));
  state.textContent = 'Counted ' + records.length + ' item(s).';
})().catch(error => { state.textContent = error.message; });
`;

test('An agent asked by the builder writes the app\'s draft, with real Goose; the builder previews and publishes', { skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 180000 }, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true, runSeconds: 90, leaseSeconds: 15 });
  const w = app.path.split('/').at(-1);
  const ravi = await app.login(1);
  const agent = { id: randomUUID(), name: 'Juno', role: 'Builds small apps', model: 'fixture', audience: 'everyone', revision: 0, settings: { provider: 'openai', instructions: 'Build what people ask for.' } };
  await app.request(`${app.path}/agent-profiles`, app.cookie, agent);
  await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  const browser = await chromium.launch({ headless: true }); const pages = []; const errors = [];
  t.after(async () => { if (!t.passed) for (const [i, page] of pages.entries()) { await page.screenshot({ path: join(app.data, `failure-${i}.png`) }).catch(() => {}); await writeFile(join(app.data, `failure-${i}.html`), await page.content()).catch(() => {}); } await browser.close(); });
  async function pageFor(cookie) { const context = await browser.newContext({ viewport: { width: 1280, height: 900 } }); const [name, value] = cookie.split('='); await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]); const page = await context.newPage(); page.on('pageerror', e => errors.push(e.message)); pages.push(page); return page; }

  const page = await pageFor(ravi);
  await page.goto(`${app.base}/w/${w}/?view=apps`);
  await page.getByRole('button', { name: 'Build an app', exact: true }).click();
  const form = page.getByRole('dialog', { name: 'Build an app' });
  await form.getByLabel('Name', { exact: true }).fill('Stock count');
  await form.getByLabel('What it is for').fill('Count the shelves');
  await form.getByRole('button', { name: 'Start building', exact: true }).click();
  await form.waitFor({ state: 'hidden' });
  const id = (await app.request(`${app.path}/apps`, ravi)).value.built[0].id;
  await page.getByRole('button', { name: 'Build with an agent', exact: true }).click();
  const ask = page.getByRole('dialog', { name: 'Build Stock count with an agent' });
  await ask.getByLabel('Agent').selectOption(agent.id);
  await ask.getByLabel('What should it build?').fill('Keep a count per item.');
  const request = `Please build my app "Stock count" (app ${id}). Keep a count per item.`;
  app.provider.setControl(request, { steps: [
    { tool: 'my_apps', arguments: {} },
    { tool: 'write_app_file', arguments: { app: id, path: 'cuckoo-app.json', content: manifest, revision: 1 } },
    { tool: 'write_app_file', arguments: { app: id, path: 'index.html', content: html, revision: 2 } },
    { tool: 'write_app_file', arguments: { app: id, path: 'app.js', content: code, revision: 3 } },
  ], checkpoint: 'The draft is ready. Preview it and publish it from Apps.' });
  await ask.getByRole('button', { name: 'Open the chat', exact: true }).click();
  await page.waitForURL(/view=chat&conversation=/);
  const chat = new URL(page.url()).searchParams.get('conversation');
  const box = page.getByRole('textbox', { name: /message/i }).first();
  await until(async () => (await box.inputValue()) === request, 'The request was not waiting in the chat box', 15000);
  app.pass('The builder asks Juno from the app; their chat with Juno opens with the request ready to send, unsent');

  const before = app.provider.requests.length;
  await page.getByRole('button', { name: 'Send message', exact: true }).click();
  const reply = await until(async () => { const h = await app.history(chat, ravi); return h.find(m => m.state === 'complete' && m.content?.includes('draft is ready')); }, 'Juno did not finish', 60000);
  assert.ok(reply);
  const prompts = app.provider.requests.slice(before).map(r => JSON.stringify(r));
  assert.ok(prompts.some(p => p.includes('This person asked you to build one or more apps in Cuckoo')), 'the builder guide is in its instructions');
  assert.ok(prompts.some(p => p.includes('ayati_control__write_app_file')), 'the app tools are offered');
  const draft = (await app.request(`${app.path}/apps/built/${id}/files?version=0`, ravi)).value;
  assert.equal(draft.draft_revision, 4);
  assert.deepEqual(draft.checks, []);
  assert.match(draft.files.find(f => f.path === 'app.js').content, /cuckoo\.create\('counts'/);
  const audit = (await app.request(`${app.path}/activity?kind=all`, app.cookie)).value.activity.map(e => e.sentence).join('\n');
  assert.match(audit, /Asked Juno to build "Stock count"/);
  assert.match(audit, /Juno changed app\.js in "Stock count"/);
  const view = (await app.request(`${app.path}/apps`, ravi)).value.built[0];
  assert.equal(view.version, null, 'the agent did not publish');
  app.pass('Real Goose: Juno writes the manifest, page and script into the draft through ayati_control; checks pass; it does not publish; audited');

  await page.goto(`${app.base}/w/${w}/?view=apps&run=${id}&preview=1`);
  const frame = page.frameLocator('iframe[title="Stock count"]');
  await frame.getByText('Counted 1 item(s).', { exact: true }).waitFor();
  await frame.getByText('Blue mugs: 12', { exact: true }).waitFor();
  await page.screenshot({ path: join(app.data, 'built-app-agent-preview.png') });
  await page.goto(`${app.base}/w/${w}/?view=apps`);
  await page.getByText('Agents building it').waitFor();
  await page.getByRole('button', { name: 'Code and publish', exact: true }).click();
  const drawer = page.getByRole('dialog', { name: 'Stock count: code' });
  await drawer.getByRole('button', { name: 'Publish', exact: true }).click();
  await drawer.getByText('Published version 1. People you chose can open it now.', { exact: true }).waitFor();
  await drawer.getByRole('button', { name: 'Close' }).click();
  await page.getByRole('button', { name: 'Stop Juno', exact: true }).click();
  await page.getByText('Agents building it').waitFor({ state: 'hidden' });
  assert.deepEqual(errors, []);
  app.pass('The builder previews the draft, which keeps a count through the bridge, publishes it, and stops Juno');
});
