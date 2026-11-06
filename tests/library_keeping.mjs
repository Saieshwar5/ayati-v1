// Keeping rules and legal holds (Library LB7) in the browser, with the real sweep: an
// admin sets "ID checks" to keep items 30 days and puts a contract in it on legal hold;
// once the ID scan and visitor log are older than that, the controller deletes them for
// good, their stored file included, while the held contract stays. No model is involved.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { writeFile, mkdtemp, access } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { randomUUID, createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { until } from './fixtures/polling.mjs';
const root = resolve(import.meta.dirname, '..');
const { chromium } = createRequire(join(root, 'web/package.json'))('playwright');
const doc = text => ({ type: 'doc', content: [{ type: 'paragraph', content: [{ type: 'text', text }] }] });
const exists = path => access(path).then(() => true, () => false);

test('Keeping rules delete old items and their files for good; a legal hold keeps an item', { skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 120000 }, async t => {
  const storage = await mkdtemp(join(root, '.tmp/library-files-'));
  const app = await workerFixture(t, { mode: 'pooled', control: true, apiEnv: { CUCKOO_LIBRARY_LOCAL_ROOT: storage, CUCKOO_LIBRARY_KEEPING_SECONDS: '5' } });
  const w = app.path.split('/').at(-1);
  const browser = await chromium.launch({ headless: true }); const pages = []; const errors = [];
  t.after(async () => { if (!t.passed) for (const [i, page] of pages.entries()) { await page.screenshot({ path: join(app.data, `failure-${i}.png`) }).catch(() => {}); await writeFile(join(app.data, `failure-${i}.html`), await page.content()).catch(() => {}); } await browser.close(); });
  async function pageFor(cookie) { const context = await browser.newContext({ viewport: { width: 1280, height: 900 } }); const [name, value] = cookie.split('='); await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]); const page = await context.newPage(); page.on('pageerror', e => errors.push(e.message)); pages.push(page); return page; }

  const folder = randomUUID(), scan = randomUUID(), log = randomUUID(), contract = randomUUID();
  await app.request(`${app.path}/library-folders`, app.cookie, { id: folder, request: randomUUID(), title: 'ID checks' });
  const bytes = Buffer.from('Asha Rao, passport X1234567');
  await app.request(`${app.path}/library-uploads`, app.cookie, { id: scan, request: randomUUID(), title: 'asha-id.txt', size: bytes.length, checksum: createHash('sha256').update(bytes).digest('hex') });
  const stored = await fetch(`${app.base}${app.path}/library-uploads/${scan}/bytes`, { method: 'POST', body: bytes, headers: { Cookie: app.cookie, Origin: app.base, 'X-Ayati-Request': 'identity', 'Content-Type': 'application/octet-stream' } });
  assert.equal(stored.status, 200, await stored.clone().text());
  await app.request(`${app.path}/library/${scan}/move`, app.cookie, { request: randomUUID(), revision: (await stored.json()).access_revision, folder });
  for (const [id, title, text] of [[log, 'Visitor log', 'Visitors on 1 October.'], [contract, 'Signed contract', 'The supplier agreement runs to 2030.']]) {
    await app.request(`${app.path}/library`, app.cookie, { id, request: randomUUID(), revision: 0, title, document: doc(text), folder });
  }
  const file = join(storage, 'library', w, scan);
  assert.ok(await exists(file), 'the scan is stored');

  const page = await pageFor(app.cookie);
  await page.goto(`${app.base}/w/${w}/?view=library&folder=${folder}`);
  await page.getByLabel('Keep items').selectOption('30');
  await until(async () => (await app.request(`${app.path}/library/${folder}/keeping`, app.cookie)).value.keep_days === 30, 'The keeping rule was not saved', 10000);
  await page.screenshot({ path: join(app.data, 'library-keeping-folder.png') });
  await page.goto(`${app.base}/w/${w}/?view=library&item=${contract}`);
  await page.getByText("Deleted for good on", { exact: false }).waitFor();
  await page.getByRole('button', { name: 'Legal hold…', exact: true }).click();
  const hold = page.getByRole('dialog', { name: 'Put Signed contract on legal hold' });
  await hold.getByLabel('Why').fill('Supplier dispute');
  await hold.getByRole('button', { name: 'Put on hold', exact: true }).click();
  await page.getByText(/On legal hold since .*: Supplier dispute/).waitFor();
  await page.screenshot({ path: join(app.data, 'library-legal-hold.png') });
  app.pass('An admin sets ID checks to keep items 30 days and puts the signed contract on legal hold in the browser');

  // Forty days later, as far as the rule is concerned.
  app.sql(`UPDATE relay_library_items SET created=created-40*86400 WHERE id IN ('${scan}','${log}','${contract}')`);
  await until(async () => app.sql(`SELECT count(*) FROM relay_library_items WHERE id IN ('${scan}','${log}') AND purged IS NOT NULL`) === '2', 'The sweep did not delete the old items', 30000);
  await until(async () => !(await exists(file)), 'The stored file was not removed', 15000);
  await app.request(`${app.path}/library/${scan}`, app.cookie, undefined, 404);
  await app.request(`${app.path}/library/${log}`, app.cookie, undefined, 404);
  const kept = (await app.request(`${app.path}/library/${contract}`, app.cookie)).value;
  assert.equal(kept.title, 'Signed contract');
  const trashed = await fetch(`${app.base}${app.path}/library/${contract}/action`, { method: 'POST', headers: { Cookie: app.cookie, Origin: app.base, 'X-Ayati-Request': 'identity', 'Content-Type': 'application/json' }, body: JSON.stringify({ request: randomUUID(), revision: kept.revision, action: 'trash' }) });
  assert.match(await trashed.text(), /legal hold/);
  await page.goto(`${app.base}/w/${w}/?view=library&folder=${folder}`);
  await page.getByRole('link', { name: 'Signed contract', exact: true }).waitFor();
  assert.equal(await page.getByRole('link', { name: 'Visitor log', exact: true }).count(), 0);
  const trash = (await app.request(`${app.path}/library?view=trash&q=&offset=0`, app.cookie)).value.items.map(i => i.id);
  assert.ok(!trash.includes(scan) && !trash.includes(log), 'not in Trash either');
  const audit = (await app.request(`${app.path}/activity?kind=all`, app.cookie)).value.activity.map(e => e.sentence).join('\n');
  assert.match(audit, /Deleted "asha-id\.txt" for good under the keeping rule of its folder/);
  assert.match(audit, /Put "Signed contract" on legal hold: Supplier dispute/);
  app.pass('The sweep deletes the old scan (its stored file too) and visitor log for good; the held contract stays and cannot be trashed; audited');

  // Emptying Trash by hand (LB7b): a receipt file and a draft page, deleted for good.
  page.on('dialog', d => void d.accept());
  const receipt = randomUUID(), draft = randomUUID();
  const receiptBytes = Buffer.from('Taxi receipt 12.50');
  await app.request(`${app.path}/library-uploads`, app.cookie, { id: receipt, request: randomUUID(), title: 'old-receipt.txt', size: receiptBytes.length, checksum: createHash('sha256').update(receiptBytes).digest('hex') });
  const sent = await fetch(`${app.base}${app.path}/library-uploads/${receipt}/bytes`, { method: 'POST', body: receiptBytes, headers: { Cookie: app.cookie, Origin: app.base, 'X-Ayati-Request': 'identity', 'Content-Type': 'application/octet-stream' } });
  const receiptItem = await sent.json();
  await app.request(`${app.path}/library`, app.cookie, { id: draft, request: randomUUID(), revision: 0, title: 'Old draft', document: doc('Never sent.') });
  await app.request(`${app.path}/library/${receipt}/action`, app.cookie, { request: randomUUID(), revision: receiptItem.revision, action: 'trash' });
  await app.request(`${app.path}/library/${draft}/action`, app.cookie, { request: randomUUID(), revision: 1, action: 'trash' });
  const receiptFile = join(storage, 'library', w, receipt);
  assert.ok(await exists(receiptFile));
  await page.goto(`${app.base}/w/${w}/?view=library`);
  await page.getByRole('button', { name: 'Trash', exact: true }).click();
  await page.getByRole('button', { name: 'Delete old-receipt.txt for good', exact: true }).click();
  await until(async () => !(await exists(receiptFile)), 'The trashed file was not removed from storage', 15000);
  await page.screenshot({ path: join(app.data, 'library-trash.png') });
  await page.getByRole('button', { name: 'Empty Trash', exact: true }).click();
  await page.getByText('Deleted 1 item for good.', { exact: true }).waitFor();
  const emptied = (await app.request(`${app.path}/library?view=trash&q=&offset=0`, app.cookie)).value.items.map(i => i.id);
  assert.ok(!emptied.includes(receipt) && !emptied.includes(draft));
  const trashAudit = (await app.request(`${app.path}/activity?kind=all`, app.cookie)).value.activity.map(e => e.sentence).join('\n');
  assert.match(trashAudit, /Deleted "old-receipt\.txt" for good from Trash/);
  assert.match(trashAudit, /Emptied Trash: 1 item deleted for good/);
  assert.deepEqual(errors, []);
  app.pass('From Trash in the browser, a receipt is deleted for good (its stored file too) and Empty Trash deletes the rest; audited');
});
