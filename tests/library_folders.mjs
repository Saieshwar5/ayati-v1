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
import { wordFile } from './fixtures/office_files.mjs';
import { createHash } from 'node:crypto';
import { mkdtemp, readFile } from 'node:fs/promises';
const root = resolve(import.meta.dirname, '..');
const { chromium } = createRequire(join(root, 'web/package.json'))('playwright');
const doc = text => ({ type: 'doc', content: [{ type: 'paragraph', content: [{ type: 'text', text }] }] });

test('A folder shared with an agent for all staff answers a colleague, with real Goose', { skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 180000 }, async t => {
  const storage = await mkdtemp(join(root, '.tmp/library-files-'));
  // Library models (LB2b) come from the providers file; it is filled in once the
  // fixture provider is listening, and read again on every call.
  const providers = join(storage, 'providers.json'), key = join(storage, 'model-key');
  await writeFile(providers, '[]', { mode: 0o600 }); await writeFile(key, 'fixture-key', { mode: 0o600 });
  const app = await workerFixture(t, { mode: 'pooled', control: true, runSeconds: 90, leaseSeconds: 15, apiEnv: { CUCKOO_LIBRARY_LOCAL_ROOT: storage, AYATI_MODEL_PROVIDERS_FILE: providers, AYATI_LIBRARY_EMBEDDING_MODEL: 'openai:fixture-embedding', AYATI_LIBRARY_SUMMARY_MODEL: 'openai:fixture-summary' } });
  await writeFile(providers, JSON.stringify([{ provider: 'openai', base_url: `${app.provider.url}/v1`, key_file: key }]), { mode: 0o600 });
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
  await sharing.getByRole('checkbox', { name: 'Iris No access' }).check();
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

  // A Word file, uploaded and filed in the shared folder, is searched by section.
  const docx = wordFile([['Chairs', 'Office chairs carry a two-year warranty.'], ['Desks', 'Standing desks carry a five-year warranty on the frame.']]);
  const upload = randomUUID();
  await app.request(`${app.path}/library-uploads`, app.cookie, { id: upload, request: randomUUID(), title: 'Warranty.docx', size: docx.length, checksum: createHash('sha256').update(docx).digest('hex') });
  const stored = await fetch(`${app.base}${app.path}/library-uploads/${upload}/bytes`, { method: 'POST', body: docx,
    headers: { Cookie: app.cookie, Origin: app.base, 'X-Ayati-Request': 'identity', 'Content-Type': 'application/octet-stream' } });
  assert.equal(stored.status, 200, await stored.clone().text());
  const file = await stored.json();
  assert.equal(file.mime, 'application/vnd.openxmlformats-officedocument.wordprocessingml.document');
  assert.equal(file.readiness, 'ready');
  await app.request(`${app.path}/library/${upload}/move`, app.cookie, { request: randomUUID(), revision: file.access_revision, folder: policies });
  const warrantyAsk = 'How long is the warranty on standing desks?';
  app.provider.setControl(warrantyAsk, { steps: [
    { tool: 'search_library', arguments: { query: 'standing desk warranty' } },
    { tool: 'read_document', arguments: { id: upload, passage: 1 } },
  ], checkpoint: 'Five years on the frame.' });
  const warranty = await app.send(warrantyAsk, chat, member, agent.id);
  const cited = await until(async () => (await app.history(chat, member)).find(m => m.id === warranty.reply && m.state === 'complete'), 'Agent did not finish', 45000);
  assert.match(cited.content, new RegExp(`\\[Warranty\\.docx › Desks · revision 1\\]\\(/w/${w}/\\?view=library&item=${upload}&source_revision=1\\)`));
  assert.equal(app.sql(`SELECT section FROM relay_library_reads WHERE reply='${warranty.reply}' AND cited`), 'Desks');
  await memberPage.goto(`${app.base}/w/${w}/?view=chat&conversation=${chat}`);
  await memberPage.getByRole('link', { name: 'Warranty.docx › Desks · revision 1', exact: true }).waitFor();
  // Filing another document is not an access change: the earlier answer stays readable.
  await memberPage.getByText('The refund window is 30 days from delivery.', { exact: true }).waitFor();
  assert.equal(await memberPage.getByText('Response unavailable because source access changed.').count(), 0);
  await memberPage.screenshot({ path: join(app.data, 'library-section-citation.png') });
  app.pass('Real Goose: an uploaded Word file is read by section and the answer cites "Warranty.docx › Desks"');

  // Meaning search: the question shares no word with the policy.
  await until(async () => Number(app.sql(`SELECT count(*) FROM relay_library_embeddings e JOIN relay_library_summaries s ON s.workspace=e.workspace AND s.item=e.item WHERE e.item='${returns}'`)) > 0, 'Library indexer did not embed and summarise the policy', 30000);
  const meaningAsk = 'Can I send something back?';
  app.provider.setControl(meaningAsk, { steps: [{ tool: 'search_library', arguments: { query: meaningAsk } }] });
  const meant = await app.send(meaningAsk, chat, member, agent.id);
  const found = await until(async () => (await app.history(chat, member)).find(m => m.id === meant.reply && m.state === 'complete'), 'Agent did not finish', 45000);
  const result = JSON.parse(JSON.parse(found.content.replace(/^CONTROL_RESULT /, '').split('\n\nLibrary documents')[0])[0]);
  assert.equal(result.by_meaning, true);
  assert.equal(result.results[0].title, 'Returns policy');
  assert.equal(result.results[0].summary, 'FIXTURE_SUMMARY Returns policy: what it covers, for colleagues.');
  assert.ok(!result.results.some(r => r.title === 'Salaries'));
  assert.ok(Number(app.sql(`SELECT count(*) FROM relay_model_usage WHERE assignment='library'`)) > 0, 'indexing usage is recorded');
  app.pass('Real Goose: "Can I send something back?" finds the returns policy by meaning, with its summary; usage is recorded');

  // Text in an image (LB2c): a photographed receipt is read by text recognition.
  const receipt = await readFile(join(root, 'tests/fixtures/library/receipt.png'));
  const image = randomUUID();
  await app.request(`${app.path}/library-uploads`, app.cookie, { id: image, request: randomUUID(), title: 'Desk receipt.png', size: receipt.length, checksum: createHash('sha256').update(receipt).digest('hex') });
  const sent = await fetch(`${app.base}${app.path}/library-uploads/${image}/bytes`, { method: 'POST', body: receipt,
    headers: { Cookie: app.cookie, Origin: app.base, 'X-Ayati-Request': 'identity', 'Content-Type': 'application/octet-stream' } });
  assert.equal(sent.status, 200, await sent.clone().text());
  const picture = await sent.json();
  assert.equal(picture.readiness, 'partial');
  await app.request(`${app.path}/library/${image}/move`, app.cookie, { request: randomUUID(), revision: picture.access_revision, folder: policies });
  const receiptAsk = 'What did the standing desk cost?';
  app.provider.setControl(receiptAsk, { steps: [{ tool: 'search_library', arguments: { query: 'standing desk receipt' } }] });
  const priced = await app.send(receiptAsk, chat, member, agent.id);
  const pricedReply = await until(async () => (await app.history(chat, member)).find(m => m.id === priced.reply && m.state === 'complete'), 'Agent did not finish', 45000);
  const pricedResult = JSON.parse(JSON.parse(pricedReply.content.replace(/^CONTROL_RESULT /, '').split('\n\nLibrary documents')[0])[0]);
  const hit = pricedResult.results.find(r => r.title === 'Desk receipt.png');
  assert.ok(hit, JSON.stringify(pricedResult));
  assert.equal(hit.section, 'Text in the image');
  assert.match(hit.preview, /1250/);
  app.pass('Real Goose: text in an uploaded receipt photo is recognised and found by search');

  // Always read (LB2c): the owner picks the policy on the agent's Instructions tab.
  await ownerPage.goto(`${app.base}/w/${w}/?view=agents&selected=${agent.id}`);
  await ownerPage.getByRole('tab', { name: 'Instructions', exact: true }).click();
  const always = ownerPage.getByRole('region', { name: 'Documents Iris always reads' });
  await always.getByLabel('Add a document').selectOption({ label: 'Policies › Returns policy' });
  await always.getByRole('button', { name: 'Add', exact: true }).click();
  await always.getByRole('button', { name: 'Save documents', exact: true }).click();
  await always.getByText('Saved.', { exact: true }).waitFor();
  await always.scrollIntoViewIfNeeded();
  await ownerPage.screenshot({ path: join(app.data, 'library-always-read.png') });
  const before = app.provider.requests.length;
  const plainAsk = 'Hello Iris';
  app.provider.setControl(plainAsk, { steps: [], checkpoint: 'Hello!' });
  const hello = await app.send(plainAsk, chat, member, agent.id);
  const greeted = await until(async () => (await app.history(chat, member)).find(m => m.id === hello.reply && m.state === 'complete'), 'Agent did not finish', 45000);
  assert.equal(greeted.content, 'Hello!', 'always-read documents are not cited');
  const prompts = app.provider.requests.slice(before).map(r => JSON.stringify(r.messages ?? []));
  assert.ok(prompts.some(p => p.includes('Company documents you always have') && p.includes('## Returns policy') && p.includes('refund window is 30 days')), 'the model received the policy');
  assert.ok(!prompts.some(p => p.includes('40k')), 'HR stays out of a colleague\'s request');
  app.pass('Real Goose: the owner chooses an always-read policy on the Instructions tab; the next request carries it, uncited, and never HR');

  // Agents changing documents (LB3): Iris may edit and save in Policies.
  const folderNow = (await app.request(`${app.path}/library/${policies}`, app.cookie)).value;
  await app.request(`${app.path}/library/${policies}/share`, app.cookie, { request: randomUUID(), revision: folderNow.access_revision, visibility: 'private', people: [], agents: [{ agent: agent.id, level: 'save', use_for: 'staff' }] });
  const ownerChat = (await app.request(`${app.path}/chats`, app.cookie, { id: randomUUID(), name: 'Owner and Iris', kind: 'group', users: [], agents: [agent.id] })).value.id;
  const policyNow = (await app.request(`${app.path}/library/${returns}`, app.cookie)).value;
  const changeAsk = 'Suggest a 45 day refund window';
  app.provider.setControl(changeAsk, { steps: [{ tool: 'suggest_edit', arguments: { id: returns, text: '# Refunds\nThe refund window is 45 days from delivery.', note: 'Asked in Chat' } }], checkpoint: 'I suggested the change.' });
  const suggested = await app.send(changeAsk, ownerChat, app.cookie, agent.id);
  await until(async () => (await app.history(ownerChat, app.cookie)).find(m => m.id === suggested.reply && m.state === 'complete'), 'Agent did not finish', 45000);
  await ownerPage.goto(`${app.base}/w/${w}/?view=library&item=${returns}`);
  await ownerPage.getByText('Iris suggested a change.', { exact: false }).waitFor();
  await ownerPage.getByRole('button', { name: 'Review', exact: true }).click();
  const review = ownerPage.getByRole('dialog', { name: 'Suggested changes' });
  await review.getByText('“Asked in Chat”', { exact: true }).waitFor();
  await ownerPage.screenshot({ path: join(app.data, 'library-suggestion-review.png') });
  await review.getByRole('button', { name: 'Accept', exact: true }).click();
  // With nothing left waiting the review closes and the page shows the accepted text.
  await review.waitFor({ state: 'hidden' });
  await ownerPage.getByText('The refund window is 45 days from delivery.', { exact: true }).waitFor();
  const accepted = (await app.request(`${app.path}/library/${returns}`, app.cookie)).value;
  assert.equal(accepted.revision, policyNow.revision + 1);
  assert.equal(accepted.document.content[0].type, 'heading');
  const history = (await app.request(`${app.path}/library/${returns}/versions`, app.cookie)).value.versions;
  assert.equal(history[0].agent, 'Iris');
  app.pass('Real Goose: Iris suggests an edit; the owner reviews it on the page and accepts it as a version marked with Iris');
  const saveAsk = 'Save a quote for Acme';
  app.provider.setControl(saveAsk, { steps: [
    { tool: 'edit_document', arguments: { id: returns, revision: accepted.revision, text: '# Refunds\nThe refund window is 60 days from delivery.' } },
    { tool: 'save_document', arguments: { folder: policies, title: 'Quote for Acme', text: '# Quote\n- Standing desk: 1250' } },
  ] });
  const saving = await app.send(saveAsk, ownerChat, app.cookie, agent.id);
  const savedReply = await until(async () => (await app.history(ownerChat, app.cookie)).find(m => m.id === saving.reply && m.state === 'complete'), 'Agent did not finish', 45000);
  const [edit, save] = JSON.parse(savedReply.content.replace(/^CONTROL_RESULT /, '')).map(r => JSON.parse(r));
  assert.equal(edit.state, 'edited');
  assert.equal(save.state, 'saved');
  const quote = (await app.request(`${app.path}/library/${save.id}`, app.cookie)).value;
  assert.equal(quote.folder, policies);
  assert.equal(quote.document.content[1].type, 'bulletList');
  const audit = (await app.request(`${app.path}/activity?kind=all`, app.cookie)).value.activity.map(e => e.sentence).join('\n');
  assert.match(audit, /Iris saved "Quote for Acme" in "Policies"/);
  app.pass('Real Goose: Iris edits the policy as a new version and saves a quote in Policies; both are audited');

  // Tables (LB4): the owner builds a price list in the browser; Iris looks a row up.
  await ownerPage.goto(`${app.base}/w/${w}/?view=library&folder=${policies}`);
  await ownerPage.getByRole('button', { name: '＋ New table', exact: true }).click();
  await ownerPage.waitForURL(/item=/);
  const priceList = new URL(ownerPage.url()).searchParams.get('item');
  await ownerPage.getByLabel('Column 1 name').fill('Product');
  await ownerPage.getByLabel('Column 2 name').fill('Price');
  await ownerPage.getByRole('button', { name: 'Add row', exact: true }).click();
  await ownerPage.getByLabel('Product, row 1').fill('Standing desk');
  await ownerPage.getByLabel('Price, row 1').fill('1250');
  await ownerPage.getByRole('button', { name: 'Save table', exact: true }).click();
  await ownerPage.getByText('Revision 2 · 1 row', { exact: false }).waitFor();
  await ownerPage.screenshot({ path: join(app.data, 'library-table.png') });
  const lookAsk = 'Price of a standing desk?';
  app.provider.setControl(lookAsk, { steps: [{ tool: 'lookup', arguments: { table: priceList, key: 'standing desk' } }] });
  const looked = await app.send(lookAsk, ownerChat, app.cookie, agent.id);
  const lookedReply = await until(async () => (await app.history(ownerChat, app.cookie)).find(m => m.id === looked.reply && m.state === 'complete'), 'Agent did not finish', 45000);
  const row = JSON.parse(JSON.parse(lookedReply.content.replace(/^CONTROL_RESULT /, '').split('\n\nLibrary documents')[0])[0]);
  assert.deepEqual(row.row, { Product: 'Standing desk', Price: 1250 });
  assert.match(lookedReply.content, /\[Untitled table › Standing desk · revision 2\]/);
  app.pass('Real Goose: the owner builds a price list in the browser; Iris looks the desk up exactly, cited by its row');
  assert.deepEqual(errors, []);
});
