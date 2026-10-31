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

test('Admin sets safety per action; a member creates an agent with tools and revocation stops pending work', { skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 180000 }, async t => {
  const remote = await remoteControlMcp(); t.after(() => remote.close());
  const directory = await mkdtemp(join(root, '.tmp/workspace-account-'));
  const profiles = join(directory, 'servers.json'); const bearer = join(directory, 'bearer');
  await writeFile(profiles, '[]', { mode: 0o600 }); await writeFile(bearer, remote.token, { mode: 0o600 });
  const app = await workerFixture(t, { mode: 'pooled', control: true, runSeconds: 90, leaseSeconds: 15, apiEnv: { AYATI_MCP_SERVERS_FILE: profiles } });
  const w = app.path.split('/').at(-1);
  await writeFile(profiles, JSON.stringify([{ id: 'orders', workspace: w, name: 'Orders sample system', account: 'Northstar test store', url: remote.url, bearer_file: bearer, operations: remote.operations }]));
  const member = await app.login(1);
  const who = (await app.request('/api/identity/session', member)).value.user;
  const cp = `${app.path}/control`;
  await app.request(`${cp}/connections`, member, undefined, 400);
  await app.request(`${cp}/systems`, member, { server: 'orders', approved: true, revision: 0 }, 400);
  const browser = await chromium.launch({ headless: true });
  t.after(async () => {
    if (!t.passed) for (const [i, context] of browser.contexts().entries()) for (const page of context.pages()) {
      await page.screenshot({ path: join(app.data, `failure-${i}.png`) }).catch(() => {});
      await writeFile(join(app.data, `failure-${i}.html`), await page.content()).catch(() => {});
    }
    await browser.close();
  });
  const errors = [];
  async function pageFor(cookie) {
    const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
    const [name, value] = cookie.split('=');
    await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
    const page = await context.newPage(); page.on('pageerror', error => errors.push(error.message)); return page;
  }
  const ownerPage = await pageFor(app.cookie); const memberPage = await pageFor(member);
  const settingsUrl = `${app.base}/w/${w}/?view=settings&scope=workspace&section=connections`;
  await memberPage.goto(settingsUrl);
  assert.equal(await memberPage.getByRole('button', { name: 'Approve system', exact: true }).count(), 0);
  await ownerPage.goto(settingsUrl);
  await ownerPage.getByRole('button', { name: 'Approve system', exact: true }).click();
  await ownerPage.getByRole('button', { name: 'Connect account', exact: true }).click();
  const safety = ownerPage.getByRole('region', { name: 'Safety for Orders sample system' });
  await safety.waitFor();
  // Everyone may create agents (D231); there is no creator permission to grant.
  assert.equal(await ownerPage.getByText('Agent creators').count(), 0);
  // Safe defaults: reads Allow, changes Ask first. The admin blocks deleting.
  const workspaceRule = async (title, mode) => (await app.request(`${cp}/connections`, app.cookie)).value.safety[0].operations.find(o => o.title === title).mode === mode;
  await until(() => workspaceRule('Update order', 'ask'), 'Write did not default to Ask first');
  await safety.getByRole('radiogroup', { name: 'Delete order workspace rule' }).getByText('Block', { exact: true }).click();
  await until(() => workspaceRule('Delete order', 'block'), 'Delete rule missing');
  await ownerPage.screenshot({ path: join(app.data, 'connector-safety-desktop.png') });
  await ownerPage.setViewportSize({ width: 390, height: 844 });
  await ownerPage.screenshot({ path: join(app.data, 'connector-safety-mobile.png') });
  assert.equal(await ownerPage.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
  await ownerPage.setViewportSize({ width: 1280, height: 900 });
  await ownerPage.evaluate(() => { document.documentElement.dataset.theme = 'dark'; });
  await ownerPage.screenshot({ path: join(app.data, 'connector-safety-dark.png') });
  await ownerPage.evaluate(() => { delete document.documentElement.dataset.theme; });
  app.pass('Admin uses Settings to approve an account and set each action\'s safety rule');

  await memberPage.goto(`${app.base}/w/${w}/?view=agents`);
  // Any member creates an agent in four steps (D233) and switches the connector on.
  await memberPage.getByRole('button', { name: '＋ New agent', exact: true }).click();
  const create = memberPage.getByRole('region', { name: 'New agent' });
  await create.getByLabel('Name', { exact: true }).fill('Orders assistant');
  await create.getByRole('button', { name: 'Continue', exact: true }).click();
  await create.getByLabel(/^Instructions/).fill('Help with sample orders using assigned operations.');
  await create.getByRole('button', { name: 'Continue', exact: true }).click();
  await create.getByRole('button', { name: 'Continue', exact: true }).click();
  await create.getByRole('switch', { name: 'Use Orders sample system' }).check();
  assert.equal(await create.getByRole('radiogroup', { name: 'Update order rule' }).getByRole('radio', { name: 'Allow' }).isDisabled(), true);
  await memberPage.screenshot({ path: join(app.data, 'create-tools-step.png') });
  await create.getByRole('button', { name: 'Create agent', exact: true }).click();
  await create.waitFor({ state: 'hidden' });
  const a = (await app.request(`${app.path}/agent-profiles`, member)).value.agents.find(a => a.name === 'Orders assistant');
  assert.equal(a.owner_id, who.id);
  const owner = (await app.request('/api/identity/session', app.cookie)).value.user;
  const chat = (await app.request(`${app.path}/chats`, member, { id: randomUUID(), kind: 'group', name: 'Order coordination', users: [owner.id], agents: [a.id] })).value.id;
  const path = `${app.path}/agent-profiles/${a.id}/control`;
  const snapshot = async () => (await app.request(path, member)).value;
  const modes = Object.fromEntries((await snapshot()).connections[0].operations.map(o => [o.id, o.mode]));
  assert.deepEqual(modes, { 'order.read': 'allow', 'order.update': 'ask', 'order.delete': 'block' });
  await memberPage.getByRole('tab', { name: 'Tools', exact: true }).click();
  const tools = memberPage.getByRole('region', { name: 'Tools' });
  assert.equal(await tools.getByRole('radiogroup', { name: 'Delete order rule' }).getByRole('radio', { name: 'Ask first' }).isDisabled(), true);
  await memberPage.screenshot({ path: join(app.data, 'agent-tools.png') });
  const connection = (await snapshot()).connections[0].id;
  // Looser than the admin's rule, or changing the admin's rule, is refused.
  await app.request(`${path}/rules`, member, { connection, operation: 'order.update', mode: 'allow', revision: 0 }, 400);
  await app.request(`${cp}/connector-rules`, member, { connection, operation: 'order.update', mode: 'allow', needs_other: false, revision: 0 }, 400);
  assert.equal(remote.calls.length, 0);
  const worker = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  const command = (operation, args) => `control ${JSON.stringify({ tool: 'request_action', arguments: { connection, operation, arguments: args } })}`;
  async function send(operation, args) {
    const job = await app.send(command(operation, args), chat, member, a.id);
    await until(async () => (await app.history(chat, member)).find(m => m.id === job.reply && (m.state === 'complete' || m.progress?.state === 'approval')), 'Agent did not finish', 35000);
    return (await snapshot()).actions.find(action => action.job === job.reply);
  }
  assert.equal((await send('order.read', { order: 'one' })).state, 'completed');
  const pending = await send('order.update', { order: 'one', address: 'Approved by its manager' });
  assert.equal(pending.state, 'pending');
  // "Ask first" goes to the agent's managers (its owner); an admin waits for the backup delay.
  await app.request(`${path}/actions/${pending.id}/decision`, app.cookie, { id: randomUUID(), decision: 'approve' }, 400);
  await app.request(`${path}/actions/${pending.id}/decision`, member, { id: randomUUID(), decision: 'approve' });
  assert.equal(remote.records.get('one').address, 'Approved by its manager');
  assert.equal((await send('order.delete', { order: 'one' })).state, 'denied');
  assert.equal(remote.calls.some(call => call.name === 'delete_order'), false);
  app.pass('Real Goose obeys the admin\'s Allow, Ask first and Block; only the agent\'s manager approves the exact write');

  const revoked = await send('order.update', { order: 'one', address: 'Must never execute' });
  await ownerPage.goto(settingsUrl);
  await safety.getByRole('radiogroup', { name: 'Update order workspace rule' }).getByText('Block', { exact: true }).click();
  await until(async () => (await snapshot()).actions.find(a => a.id === revoked.id)?.state === 'invalidated', 'Revocation did not invalidate action');
  await app.request(`${path}/actions/${revoked.id}/decision`, member, { id: randomUUID(), decision: 'approve' }, 400);
  assert.equal(remote.records.get('one').address, 'Approved by its manager');
  await app.stopApi(); await app.startApi();
  assert.equal((await snapshot()).connections[0].operations.find(o => o.id === 'order.update').mode, 'block');
  await writeFile(profiles, '{ invalid operator configuration');
  await ownerPage.goto(settingsUrl);
  await ownerPage.getByRole('alert').filter({ hasText: 'Account configuration is unavailable' }).waitFor();
  await ownerPage.getByRole('button', { name: 'Disconnect', exact: true }).click();
  await until(async () => !(await app.request(`${cp}/connections`, app.cookie)).value.connections[0].enabled, 'Broken configuration prevented disconnect');
  app.pass('Broken operator configuration still permits account disconnection and revocation');
  assert.deepEqual(errors, []);
  assert.equal(app.processes().some(p => p.logs.includes(remote.token)), false);
  assert.equal(JSON.stringify(app.provider.requests).includes(remote.token), false);
  await app.stop(worker);
  app.pass('UI revocation invalidates pending approval and survives restart without an external write or credential leak');
});
