import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, readFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { randomUUID } from 'node:crypto';
import { spawn } from 'node:child_process';
import { createRequire } from 'node:module';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { remoteControlMcp } from './fixtures/remote_control_mcp.mjs';
import { until } from './fixtures/polling.mjs';
const root = resolve(import.meta.dirname, '..');
const { chromium } = createRequire(join(root, 'web/package.json'))('playwright');

test('shared real Goose obeys the admin safety rules and the agent Tools switch, with exact approvals and retained outcomes', { skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 180000 }, async t => {
  const remote = await remoteControlMcp(); t.after(() => remote.close());
  const privateDir = await mkdtemp(join(root, '.tmp/control-profile-'));
  const profileFile = join(privateDir, 'servers.json'); const bearerFile = join(privateDir, 'bearer');
  await writeFile(profileFile, '[]', { mode: 0o600 }); await writeFile(bearerFile, remote.token, { mode: 0o600 });
  const app = await workerFixture(t, { mode: 'pooled', control: true, runSeconds: 90, leaseSeconds: 15,
    apiEnv: { AYATI_MCP_SERVERS_FILE: profileFile } });
  const workspace = app.path.split('/').at(-1);
  t.after(() => writeFile(join(app.data, 'control-model-requests.json'), JSON.stringify(app.provider.requests, null, 2), { mode: 0o600 }));
  async function done(job) {
    const message = await until(async () => (await app.history(job.chat)).find(m => m.id === job.reply && (['complete', 'failed', 'stopped'].includes(m.state) || m.progress?.state === 'approval')), 'Agent did not reach a terminal state', 35000);
    assert.ok(message.state === 'complete' || message.progress?.state === 'approval', message.content);
    return message;
  }
  await writeFile(profileFile, JSON.stringify([{ id: 'orders', workspace, name: 'Orders sample system', url: remote.url, bearer_file: bearerFile, operations: remote.operations }]));
  const path = `${app.path}/agent-profiles/${app.profile.id}/control`;
  const snapshot = async () => (await app.request(path, app.cookie)).value;
  const browser = await chromium.launch({ headless: true }); t.after(() => browser.close());
  const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  const [name, value] = app.cookie.split('=');
  await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
  const page = await context.newPage(); const errors = []; page.on('pageerror', error => errors.push(error.message));
  await page.goto(`${app.base}/w/${workspace}/?view=settings&scope=workspace&section=connections`);
  await page.getByRole('button', { name: 'Approve system', exact: true }).click();
  await page.getByRole('button', { name: 'Connect account', exact: true }).click();
  // Admin safety (D231): reads Allow and changes Ask first by default; deleting is blocked here.
  const safety = page.getByRole('region', { name: 'Safety for Orders sample system' });
  await safety.getByRole('radiogroup', { name: 'Delete order workspace rule' }).getByText('Block', { exact: true }).click();
  await until(async () => (await app.request(`${app.path}/control/connections`, app.cookie)).value.safety[0].operations.find(o => o.id === 'order.delete').mode === 'block', 'Workspace rule did not persist');
  await page.screenshot({ path: join(app.data, 'connector-safety-desktop.png') });
  // The agent's owner switches the connector on from the Tools tab.
  await page.goto(`${app.base}/w/${workspace}/?view=agents`);
  await page.getByRole('tab', { name: 'Tools', exact: true }).click();
  const tools = page.getByRole('region', { name: 'Tools' });
  await tools.getByRole('switch', { name: 'Use Orders sample system' }).click();
  await until(async () => (await snapshot()).connections[0]?.on === true, 'Tools switch did not persist');
  const modes = Object.fromEntries((await snapshot()).connections[0].operations.map(o => [o.id, o.mode]));
  assert.deepEqual(modes, { 'order.read': 'allow', 'order.update': 'ask', 'order.delete': 'block' });
  await tools.getByRole('radiogroup', { name: 'Update order rule' }).getByText('Allow', { exact: true }).waitFor();
  assert.equal(await tools.getByRole('radiogroup', { name: 'Update order rule' }).getByRole('radio', { name: 'Allow' }).isDisabled(), true);
  const connection = (await snapshot()).connections[0].id;
  assert.equal(remote.calls.length, 0, 'Discovery must not execute any business operation');
  const worker = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  await until(() => Number(app.sql("SELECT count(*) FROM relay_executor_slots WHERE state='ready'")) >= 1, 'Prepared slot did not become ready', 35000);
  const command = (operation, args) => `control ${JSON.stringify({ tool: 'request_action', arguments: { connection, operation, arguments: args } })}`;
  const read = await app.send(command('order.read', { order: 'one' })); await done(read);
  assert.equal(remote.calls.filter(c => c.name === 'read_order').length, 1);
  assert.equal((await snapshot()).actions.find(a => a.operation === 'order.read').state, 'completed');
  app.pass('real Goose invokes an allowed read over authenticated Streamable HTTP MCP');

  const write = await app.send(command('order.update', { order: 'one', address: 'Reviewed address' })); await done(write);
  const pending = (await snapshot()).actions.find(a => a.job === write.reply);
  assert.equal(pending.state, 'pending'); assert.equal(remote.records.get('one').address, 'Old address');
  const member = await app.login(1);
  await app.request(`${path}/actions/${pending.id}/decision`, member, { id: randomUUID(), decision: 'approve' }, 400);
  // The owner manages Nova, so they decide from its Tools tab.
  await page.reload(); await page.getByRole('tab', { name: 'Tools', exact: true }).click();
  await page.getByRole('region', { name: 'Recent tool actions' }).getByRole('button', { name: 'Details' }).first().click();
  const drawer = page.getByRole('dialog', { name: 'Review approval request' });
  await drawer.getByRole('button', { name: 'Approve this action' }).waitFor();
  await page.screenshot({ path: join(app.data, 'pending-approval-desktop.png') });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({ path: join(app.data, 'pending-approval-mobile.png') });
  await page.setViewportSize({ width: 1280, height: 900 });
  await drawer.getByRole('button', { name: 'Approve this action' }).click();
  await until(async () => (await snapshot()).actions.find(a => a.id === pending.id)?.state === 'completed', 'Approved action did not complete');
  assert.equal(remote.records.get('one').address, 'Reviewed address');
  await Promise.all([1, 2].map(() => app.request(`${path}/actions/${pending.id}/decision`, app.cookie, { id: randomUUID(), decision: 'approve' })));
  assert.equal(remote.calls.filter(c => c.name === 'update_order').length, 1);
  app.pass('UI exact approval by a manager executes once; a non-manager and repeated decisions cannot dispatch another write');

  const denied = await app.send(command('order.delete', { order: 'one' })); await done(denied);
  assert.equal((await snapshot()).actions.find(a => a.job === denied.reply).state, 'denied');
  assert.equal(remote.calls.some(c => c.name === 'delete_order'), false);
  const staleJob = await app.send(command('order.update', { order: 'one', address: 'Stale address' })); await done(staleJob);
  const stale = (await snapshot()).actions.find(a => a.job === staleJob.reply);
  // The agent's own rule may be stricter than the workspace's.
  await app.request(`${path}/rules`, app.cookie, { connection, operation: 'order.update', mode: 'block', revision: 0 });
  await app.request(`${path}/actions/${stale.id}/decision`, app.cookie, { id: randomUUID(), decision: 'approve' }, 400);
  assert.equal((await snapshot()).actions.find(a => a.id === stale.id).state, 'invalidated');
  app.pass('default block and rule revocation prevent external effects and invalidate stale approval');

  // Allow needs the admin's rule to allow it first; then the agent may allow it too.
  await app.request(`${app.path}/control/connector-rules`, app.cookie, { connection, operation: 'order.update', mode: 'allow', needs_other: false, revision: 0 });
  await app.request(`${path}/rules`, app.cookie, { connection, operation: 'order.update', mode: 'allow', revision: 1 });
  const lost = await app.send(command('order.update', { order: 'lost', address: 'Applied but unconfirmed' })); await done(lost);
  assert.equal((await snapshot()).actions.find(a => a.job === lost.reply).state, 'uncertain');
  assert.equal(remote.records.get('lost').address, 'Applied but unconfirmed');
  assert.equal(remote.calls.filter(c => c.arguments.order === 'lost').length, 1);
  await app.stopApi(); await app.startApi();
  assert.equal((await snapshot()).actions.find(a => a.job === lost.reply).state, 'uncertain');
  app.pass('lost write response is retained as uncertain across controller restart, without automatic retry');

  await page.reload();
  await page.getByRole('tab', { name: 'Tools', exact: true }).click();
  await page.getByRole('region', { name: 'Recent tool actions' }).waitFor();
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({ path: join(app.data, 'tools-approvals-mobile.png') });
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth), false);
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.screenshot({ path: join(app.data, 'tools-approvals-desktop.png') });
  await page.evaluate(() => { document.documentElement.dataset.theme = 'dark'; });
  await page.screenshot({ path: join(app.data, 'tools-approvals-dark.png') });
  assert.deepEqual(errors, []);
  const leaked = app.processes().some(p => p.logs.includes(remote.token)); assert.equal(leaked, false);
  assert.equal(JSON.stringify(app.provider.requests).includes(remote.token), false);
  assert.equal((await readFile(join(privateDir, 'bearer'), 'utf8')), remote.token);
  await app.stop(worker);
  app.pass('desktop/mobile control journey, durable receipts and credential filtering');
});
