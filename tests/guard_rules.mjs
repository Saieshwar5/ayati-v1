// Company rules (the Guard, core-agent CA1) with real Goose: an admin writes rules in
// Settings, the controller holds or blocks the agent's exact requests on top of an
// Allow, and the approval says which rule and the facts it used. The model is
// scripted; Goose, the controller, ayati_control and the MCP account are real.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { remoteControlMcp } from './fixtures/remote_control_mcp.mjs';
import { until } from './fixtures/polling.mjs';
const root = resolve(import.meta.dirname, '..');
const { chromium } = createRequire(join(root, 'web/package.json'))('playwright');

test('Company rules written in Settings hold and block an allowed agent action, with real Goose', { skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 180000 }, async t => {
  const remote = await remoteControlMcp(); t.after(() => remote.close());
  const directory = await mkdtemp(join(root, '.tmp/guard-profile-'));
  const profiles = join(directory, 'servers.json'), bearer = join(directory, 'bearer');
  await writeFile(profiles, '[]', { mode: 0o600 }); await writeFile(bearer, remote.token, { mode: 0o600 });
  const app = await workerFixture(t, { mode: 'pooled', control: true, runSeconds: 90, leaseSeconds: 15, apiEnv: { AYATI_MCP_SERVERS_FILE: profiles } });
  const w = app.path.split('/').at(-1), connection = randomUUID();
  await writeFile(profiles, JSON.stringify([{ id: 'orders', workspace: w, name: 'Company orders', account: 'Northstar test store', url: remote.url, bearer_file: bearer, operations: remote.operations }]));
  const member = await app.login(1);
  const who = async cookie => (await app.request('/api/identity/session', cookie)).value.user;
  const [owner, creator] = await Promise.all([who(app.cookie), who(member)]);
  await app.request(`${app.path}/control/systems`, app.cookie, { server: 'orders', approved: true, revision: 0 });
  await app.request(`${app.path}/control/connections`, app.cookie, { id: connection, server: 'orders', enabled: true, revision: 0 });
  // The admin allows changing an order without asking: the company rules sit on top.
  await app.request(`${app.path}/control/connector-rules`, app.cookie, { connection, operation: 'order.update', mode: 'allow', needs_other: false, revision: 0 });
  const agent = { id: randomUUID(), name: 'Order support', role: 'Order support', model: 'fixture', audience: 'everyone', managers: [creator.id], revision: 0, settings: { provider: 'openai', instructions: 'Use assigned order operations.' } };
  await app.request(`${app.path}/agent-profiles`, member, agent);
  const control = `${app.path}/agent-profiles/${agent.id}/control`;
  await app.request(`${control}/tools`, member, { connection, enabled: true });
  const chat = (await app.request(`${app.path}/chats`, member, { id: randomUUID(), name: 'Order work', kind: 'group', users: [owner.id], agents: [agent.id] })).value.id;
  // Members can read rules but not write them.
  await app.request(`${app.path}/guard/rules`, member, { id: randomUUID(), operation: 'order.update', connection, kind: 'field', field: 'address', comparator: '=', value: 'x', effect: 'block', note: '', enabled: true, revision: 0 }, 400);

  const browser = await chromium.launch({ headless: true }); const pages = []; const errors = [];
  t.after(async () => { if (!t.passed) for (const [i, page] of pages.entries()) { await page.screenshot({ path: join(app.data, `failure-${i}.png`) }).catch(() => {}); await writeFile(join(app.data, `failure-${i}.html`), await page.content()).catch(() => {}); } await browser.close(); });
  async function pageFor(cookie) { const context = await browser.newContext({ viewport: { width: 1280, height: 900 } }); const [name, value] = cookie.split('='); await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]); const page = await context.newPage(); page.on('pageerror', e => errors.push(e.message)); pages.push(page); return page; }
  const ownerPage = await pageFor(app.cookie), memberPage = await pageFor(member);
  const rulesUrl = `${app.base}/w/${w}/?view=settings&section=company-rules`;
  await ownerPage.goto(rulesUrl);
  const rules = ownerPage.getByRole('region', { name: 'Company rules' });
  async function addRule(value, effect) {
    await rules.getByRole('button', { name: 'Add rule', exact: true }).click();
    const dialog = ownerPage.getByRole('dialog', { name: 'Add a company rule' });
    await dialog.getByLabel('Applies to').selectOption({ label: 'Company orders · Update order' });
    await dialog.getByLabel('Field').selectOption('address');
    await dialog.getByLabel('Value').fill(value);
    await dialog.getByLabel(effect, { exact: true }).check();
    await dialog.getByText(`Update order: if address is "${value}", ${effect === 'Block' ? 'block it' : 'ask first'}.`, { exact: true }).waitFor();
    await dialog.getByRole('button', { name: 'Save rule', exact: true }).click();
    await dialog.waitFor({ state: 'hidden' });
  }
  await addRule('Check with Ravi', 'Ask first');
  await addRule('Blocked street', 'Block');
  await rules.getByText('Update order: if address is "Blocked street", block it.', { exact: true }).waitFor();
  await ownerPage.screenshot({ path: join(app.data, 'company-rules-desktop.png') });
  await ownerPage.setViewportSize({ width: 390, height: 844 });
  await ownerPage.screenshot({ path: join(app.data, 'company-rules-mobile.png') });
  assert.equal(await ownerPage.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
  await ownerPage.setViewportSize({ width: 1280, height: 900 });
  await ownerPage.evaluate(() => { document.documentElement.dataset.theme = 'dark'; });
  await ownerPage.screenshot({ path: join(app.data, 'company-rules-dark.png') });
  await memberPage.goto(rulesUrl);
  await memberPage.getByText('Update order: if address is "Check with Ravi", ask first.', { exact: true }).waitFor();
  assert.equal(await memberPage.getByRole('button', { name: 'Add rule' }).count(), 0);
  const audit = (await app.request(`${app.path}/activity?kind=all`, app.cookie)).value.activity.map(e => e.sentence).join('\n');
  assert.match(audit, /Saved company rule: Update order: if address is "Blocked street", block it\./);
  app.pass('An admin writes company rules in Settings with a plain-words read-back; members read them; saves are audited');

  const snapshot = async () => (await app.request(control, member)).value;
  const worker = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  async function propose(address) {
    const command = `control ${JSON.stringify({ tool: 'request_action', arguments: { connection, operation: 'order.update', arguments: { order: 'one', address } } })}`;
    const job = await app.send(command, chat, member, agent.id);
    await until(async () => (await app.history(chat, member)).find(m => m.id === job.reply && (m.state === 'complete' || m.progress?.state === 'approval')), 'Agent did not finish', 35000);
    return (await snapshot()).actions.find(a => a.job === job.reply);
  }
  assert.equal((await propose('Normal street')).state, 'completed');
  assert.equal(remote.records.get('one').address, 'Normal street');
  const blocked = await propose('Blocked street');
  assert.equal(blocked.state, 'denied');
  assert.equal(blocked.guard.effect, 'block');
  assert.equal(remote.records.get('one').address, 'Normal street', 'nothing was sent');
  const held = await propose('Check with Ravi');
  assert.equal(held.state, 'pending');
  assert.equal(held.guard.summary, 'Update order: if address is "Check with Ravi", ask first.');
  assert.equal(remote.records.get('one').address, 'Normal street');
  app.pass('Real Goose: an allowed write runs, a blocked one is refused and a held one waits, with nothing sent');

  await memberPage.goto(`${app.base}/w/${w}/?view=chat&conversation=${chat}`);
  await memberPage.getByRole('button', { name: 'Review and decide', exact: true }).click();
  const review = memberPage.getByRole('dialog', { name: 'Review approval request' });
  await review.getByText('Held by a company rule', { exact: true }).waitFor();
  assert.match(await review.innerText(), /This request's address: Check with Ravi\./);
  await memberPage.screenshot({ path: join(app.data, 'company-rule-approval.png') });
  await review.getByRole('button', { name: 'Approve this action', exact: true }).click();
  await until(async () => remote.records.get('one').address === 'Check with Ravi', 'Approved write missing');
  app.pass('The approval names the rule and its facts; the agent\'s manager approves the exact write once');
  // Read before you change: Cuckoo's own receipt of the read, in the same task.
  await app.request(`${app.path}/guard/rules`, app.cookie, { id: randomUUID(), operation: 'order.update', connection, kind: 'read_first', field: '', comparator: '=', value: true, effect: 'block', note: '', enabled: true, revision: 0, source: { operation: 'order.read', match: 'order' } });
  async function run(ask, steps) {
    app.provider.setControl(ask, { steps });
    const job = await app.send(ask, chat, member, agent.id);
    await until(async () => (await app.history(chat, member)).find(m => m.id === job.reply && (m.state === 'complete' || m.progress?.state === 'approval')), 'Agent did not finish', 35000);
    return (await snapshot()).actions.filter(a => a.job === job.reply);
  }
  const update = address => ({ tool: 'request_action', arguments: { connection, operation: 'order.update', arguments: { order: 'one', address } } });
  const [blindly] = await run('Change the address without looking', [update('Unread street')]);
  assert.equal(blindly.state, 'denied');
  assert.equal(blindly.guard.facts.found, false);
  assert.equal(remote.records.get('one').address, 'Check with Ravi');
  const careful = await run('Read the order, then change the address', [{ tool: 'request_action', arguments: { connection, operation: 'order.read', arguments: { order: 'one' } } }, update('Read street')]);
  assert.deepEqual(careful.map(a => [a.operation, a.state]).sort(), [['order.read', 'completed'], ['order.update', 'completed']]);
  assert.equal(remote.records.get('one').address, 'Read street');
  app.pass('Real Goose: a change without reading the order first is blocked; after reading it in the same task, it runs');
  // An agent proposes a rule from the person's words; it isn't in force until an admin
  // reviews it, tries it on past requests and turns it on.
  await run('Make a rule: moving an order to Lake street needs my approval', [{ tool: 'propose_rule', arguments: { words: 'Moving an order to Lake street needs my approval', operation: 'order.update', connection, kind: 'field', field: 'address', comparator: '=', value: 'Lake street', effect: 'ask' } }]);
  const proposedRule = (await app.request(`${app.path}/guard/rules`, app.cookie)).value.rules.find(r => !r.reviewed);
  assert.equal(proposedRule.summary, 'Update order: if address is "Lake street", ask first.');
  assert.equal(proposedRule.enabled, false);
  await ownerPage.goto(rulesUrl);
  const proposals = ownerPage.getByRole('region', { name: 'Proposed by agents' });
  await proposals.getByText('Order support proposed it from', { exact: false }).waitFor();
  await proposals.getByRole('button', { name: 'Review', exact: true }).click();
  const reviewing = ownerPage.getByRole('dialog', { name: 'Review proposed rule' });
  await reviewing.getByRole('button', { name: 'Try it on the last 30 days', exact: true }).click();
  await reviewing.getByText(/matching requests|No matching requests/).waitFor();
  await ownerPage.screenshot({ path: join(app.data, 'company-rule-review.png') });
  await reviewing.getByRole('button', { name: 'Turn on', exact: true }).click();
  await reviewing.waitFor({ state: 'hidden' });
  const readOne = { tool: 'request_action', arguments: { connection, operation: 'order.read', arguments: { order: 'one' } } };
  const lake = await run('Read the order and move it to Lake street', [readOne, update('Lake street')]);
  assert.equal(lake.find(a => a.operation === 'order.update').state, 'pending');
  assert.equal(remote.records.get('one').address, 'Read street');
  app.pass('Real Goose proposes a rule from a person\'s words; an admin reviews, tries and turns it on; it then holds the matching change');
  assert.deepEqual(errors, []);
  await app.stop(worker);
});
