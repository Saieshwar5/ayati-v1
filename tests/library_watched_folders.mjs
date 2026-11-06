// Watched folders (Library LB6) with real Goose: the owner asks Iris to watch Inbox; an
// invoice uploaded there becomes Iris's work in its own conversation, which records the
// amount and due date, opens a "pay?" task and files it under Invoices. The model is
// scripted; Goose, the controller, the dispatcher and ayati_control are real.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { writeFile, mkdtemp } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { randomUUID, createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { until } from './fixtures/polling.mjs';
const root = resolve(import.meta.dirname, '..');
const { chromium } = createRequire(join(root, 'web/package.json'))('playwright');

test('An invoice uploaded to a watched folder is recorded, tasked and filed by the agent, with real Goose', { skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 180000 }, async t => {
  const storage = await mkdtemp(join(root, '.tmp/library-files-'));
  const app = await workerFixture(t, { mode: 'pooled', control: true, runSeconds: 90, leaseSeconds: 15, apiEnv: { CUCKOO_LIBRARY_LOCAL_ROOT: storage } });
  const w = app.path.split('/').at(-1);
  const agent = { id: randomUUID(), name: 'Iris', role: 'Handles paperwork', model: 'fixture', audience: 'everyone', revision: 0, settings: { provider: 'openai', instructions: 'Handle paperwork carefully.' } };
  await app.request(`${app.path}/agent-profiles`, app.cookie, agent);
  const inbox = randomUUID(), invoices = randomUUID();
  for (const [id, title, level] of [[inbox, 'Inbox', 'suggest'], [invoices, 'Invoices', 'save']]) {
    await app.request(`${app.path}/library-folders`, app.cookie, { id, request: randomUUID(), title });
    await app.request(`${app.path}/library/${id}/share`, app.cookie, { request: randomUUID(), revision: 1, visibility: 'private', people: [], agents: [{ agent: agent.id, level, use_for: 'staff' }] });
  }
  await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  const browser = await chromium.launch({ headless: true }); const pages = []; const errors = [];
  t.after(async () => { if (!t.passed) for (const [i, page] of pages.entries()) { await page.screenshot({ path: join(app.data, `failure-${i}.png`) }).catch(() => {}); await writeFile(join(app.data, `failure-${i}.html`), await page.content()).catch(() => {}); } await browser.close(); });
  async function pageFor(cookie) { const context = await browser.newContext({ viewport: { width: 1280, height: 900 } }); const [name, value] = cookie.split('='); await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]); const page = await context.newPage(); page.on('pageerror', e => errors.push(e.message)); pages.push(page); return page; }

  const page = await pageFor(app.cookie);
  await page.goto(`${app.base}/w/${w}/?view=library&folder=${inbox}`);
  await page.getByRole('button', { name: 'Watch with an agent', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Watch Inbox with an agent' });
  await dialog.getByLabel('Agent').selectOption(agent.id);
  await dialog.getByLabel('What should it do with each new item?').fill('Record the supplier, amount and due date, ask me whether to pay, and file it under Invoices.');
  await dialog.getByRole('button', { name: 'Save', exact: true }).click();
  await page.getByText(/Iris handles new items here for/).waitFor();
  await page.screenshot({ path: join(app.data, 'library-watched-folder.png') });
  const watcher = (await app.request(`${app.path}/library/${inbox}/watch`, app.cookie)).value.watcher;
  app.pass('The owner asks Iris, given the folder, to handle new items in Inbox from its header');

  const invoice = randomUUID();
  const bytes = Buffer.from('Invoice 42 from Acme Ltd. Amount due: 1,250. Pay by 1 December 2026.');
  app.provider.setControl('New in the Library folder "Inbox": invoice-42.txt', { steps: [
    { tool: 'read_document', arguments: { id: invoice } },
    { tool: 'set_document_fields', arguments: { id: invoice, fields: { supplier: 'Acme Ltd', amount: 1250, due: '2026-12-01' } } },
    { tool: 'create_task', arguments: { title: 'Pay Acme invoice 42?', done_when: 'The owner decides whether to pay 1,250 by 1 December' } },
    { tool: 'file_document', arguments: { id: invoice, folder: invoices } },
  ], checkpoint: 'Filed invoice 42 from Acme: 1,250 due 1 December. Shall I pay it?' });
  await app.request(`${app.path}/library-uploads`, app.cookie, { id: invoice, request: randomUUID(), title: 'invoice-42.txt', size: bytes.length, checksum: createHash('sha256').update(bytes).digest('hex') });
  const stored = await fetch(`${app.base}${app.path}/library-uploads/${invoice}/bytes`, { method: 'POST', body: bytes, headers: { Cookie: app.cookie, Origin: app.base, 'X-Ayati-Request': 'identity', 'Content-Type': 'application/octet-stream' } });
  assert.equal(stored.status, 200, await stored.clone().text());
  await app.request(`${app.path}/library/${invoice}/move`, app.cookie, { request: randomUUID(), revision: (await stored.json()).access_revision, folder: inbox });
  const reply = await until(async () => (await app.history(watcher.chat, app.cookie)).find(m => m.state === 'complete' && m.content?.includes('Shall I pay it?')), 'Iris did not handle the invoice', 60000);
  assert.ok(reply);
  const filed = (await app.request(`${app.path}/library/${invoice}`, app.cookie)).value;
  assert.equal(filed.folder, invoices);
  const fields = (await app.request(`${app.path}/library/${invoice}/fields`, app.cookie)).value.fields.fields;
  assert.deepEqual(fields, { supplier: 'Acme Ltd', amount: 1250, due: '2026-12-01' });
  assert.equal(app.sql(`SELECT count(*) FROM relay_tasks WHERE title='Pay Acme invoice 42?'`), '1');
  app.pass('Real Goose: the upload becomes Iris\'s work; it records supplier, amount and due date, opens a "pay?" task and files it under Invoices');

  await page.goto(`${app.base}/w/${w}/?view=library&item=${invoice}`);
  await page.getByRole('region', { name: 'Key fields' }).getByText('1250').waitFor();
  await page.screenshot({ path: join(app.data, 'library-key-fields.png') });
  await page.goto(`${app.base}/w/${w}/?view=chat&conversation=${watcher.chat}`);
  await page.getByText('Shall I pay it?', { exact: false }).first().waitFor();
  const audit = (await app.request(`${app.path}/activity?kind=all`, app.cookie)).value.activity.map(e => e.sentence).join('\n');
  assert.match(audit, /Iris filed "invoice-42\.txt" in "Invoices"/);
  assert.match(audit, /Iris recorded supplier, amount, due on "invoice-42\.txt"/);
  assert.deepEqual(errors, []);
  app.pass('The invoice shows its key fields, the work is in Chat, and filing and fields are audited');
});
