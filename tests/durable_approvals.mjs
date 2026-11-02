import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { remoteControlMcp } from './fixtures/remote_control_mcp.mjs';
import { until } from './fixtures/polling.mjs';
import { cancelRequest } from './fixtures/chat_actions.mjs';

const root = resolve(import.meta.dirname, '..');
const { chromium } = createRequire(join(root, 'web/package.json'))('playwright');

test('saved task releases execution for approval and continues on a different executor across controller restarts', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 240000,
}, async t => {
  const remote = await remoteControlMcp();
  t.after(() => remote.close());
  const directory = await mkdtemp(join(root, '.tmp/continuation-profile-'));
  const profiles = join(directory, 'servers.json'), bearer = join(directory, 'bearer');
  await writeFile(profiles, '[]', { mode: 0o600 });
  await writeFile(bearer, remote.token, { mode: 0o600 });
  const app = await workerFixture(t, { mode: 'pooled', control: true, runSeconds: 90,
    leaseSeconds: 15, capacity: 1, workspaceLimit: 1, modelLimit: 1,
    apiEnv: { AYATI_MCP_SERVERS_FILE: profiles } });
  const w = app.path.split('/').at(-1), connection = randomUUID();
  await writeFile(profiles, JSON.stringify([{ id: 'orders', workspace: w, name: 'Company orders',
    account: 'Test company', url: remote.url, bearer_file: bearer, operations: remote.operations }]));
  await app.request(`${app.path}/control/systems`, app.cookie, { server: 'orders', approved: true, revision: 0 });
  await app.request(`${app.path}/control/connections`, app.cookie, { id: connection, server: 'orders', enabled: true, revision: 0 });
  const control = `${app.path}/agent-profiles/${app.profile.id}/control`;
  // The admin's rules for this connector, then the agent's Tools switch (schema 47).
  for (const [operation, mode] of [['order.read', 'allow'], ['order.update', 'ask']]) {
    await app.request(`${app.path}/control/connector-rules`, app.cookie, { connection, operation, mode, needs_other: false, revision: 0 });
  }
  await app.request(`${control}/tools`, app.cookie, { connection, enabled: true });
  const snapshot = async () => (await app.request(control, app.cookie)).value;
  const jobState = job => JSON.parse(app.sql(`SELECT row_to_json(j) FROM relay_chat_jobs j WHERE reply='${job.reply}'`));
  const actionFor = async job => (await snapshot()).actions.find(a => a.job === job.reply && a.operation === 'order.update');
  const argumentsFor = (address, order = 'one') => ({ connection, operation: 'order.update', arguments: { order, address } });
  const command = (address, order = 'one', after = []) => `control ${JSON.stringify({
    tool: 'request_action', arguments: argumentsFor(address, order), after,
    checkpoint: 'The address change is saved for review. After approval I will check the updated order and finish the summary.',
  })}`;
  async function waiting(job) {
    await until(async () => (await app.history()).find(m => m.id === job.reply && m.progress?.state === 'approval'), 'Task did not save its approval wait', 35000);
    assert.equal(jobState(job).state, 'waiting');
    assert.equal(jobState(job).lease_until, 0);
    return actionFor(job);
  }
  const browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  const [cookieName, cookieValue] = app.cookie.split('=');
  await context.addCookies([{ name: cookieName, value: cookieValue, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
  const page = await context.newPage(), errors = [];
  page.on('pageerror', error => errors.push(error.message));
  t.after(async () => {
    if (!t.passed) {
      await page.screenshot({ path: join(app.data, 'failure.png') }).catch(() => {});
      await writeFile(join(app.data, 'failure.html'), await page.content()).catch(() => {});
    }
    await writeFile(join(app.data, 'model-requests.json'), JSON.stringify(app.provider.requests, null, 2), { mode: 0o600 });
    await browser.close();
  });

  const firstWorker = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  const address = 'APPROVED_FOR_ORIGINAL_TASK';
  const after = [
    { tool: 'request_action', arguments: argumentsFor(address) }, // Exact replay must not write again.
    { tool: 'request_action', arguments: { connection, operation: 'order.read', arguments: { order: 'one' } } },
  ];
  const original = await app.send(command(address, 'one', after));
  const proposal = await waiting(original);
  assert.equal(remote.calls.length, 0);
  const firstClaim = jobState(original).claim;
  assert.equal(app.sql(`SELECT state FROM relay_worker_assignments WHERE id='${firstClaim}'`), 'finished');
  assert.equal(Number(app.sql(`SELECT count(*) FROM relay_task_checkpoints WHERE reply='${original.reply}'`)), 1);
  await page.goto(`${app.base}/w/${w}/?view=chat&conversation=${app.conversation}`);
  await page.locator('.chat-reply-progress [role=status]').filter({ hasText: 'Waiting for approval' }).waitFor();
  await page.locator('.chat-reply-progress [role=status]').filter({ hasText: 'Waiting for approval' }).scrollIntoViewIfNeeded();
  await page.screenshot({ path: join(app.data, 'waiting-desktop.png') });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.locator('.chat-reply-progress [role=status]').filter({ hasText: 'Waiting for approval' }).scrollIntoViewIfNeeded();
  await page.screenshot({ path: join(app.data, 'waiting-mobile.png') });
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
  await page.setViewportSize({ width: 1280, height: 900 });
  const other = await app.send('Another useful request while the first waits');
  await app.done(other);
  assert.equal(jobState(original).state, 'waiting');
  const callsWhileWaiting = app.provider.requests.length;
  await until(() => Number(app.sql("SELECT count(*) FROM relay_executor_slots WHERE state='ready'")) === 1, 'Waiting task held the worker', 35000);
  await new Promise(resolve => setTimeout(resolve, 1200));
  assert.equal(app.provider.requests.length, callsWhileWaiting, 'A human wait started a model polling loop');
  app.pass('A durable original-task checkpoint releases its worker, permits another task on the same agent and makes no model calls while waiting');

  await app.stop(firstWorker);
  await app.stopApi(); await app.startApi();
  await page.reload();
  await page.locator('.chat-reply-progress [role=status]').filter({ hasText: 'Waiting for approval' }).waitFor();
  await page.getByRole('button', { name: 'Review and decide', exact: true }).click();
  const review = page.getByRole('dialog', { name: 'Review approval request' });
  await review.getByRole('button', { name: 'Approve this action', exact: true }).click();
  await until(() => jobState(original).state === 'queued', 'Saved approval did not queue continuation');
  assert.equal(remote.records.get('one').address, address);
  await app.request(`${control}/actions/${proposal.id}/decision`, app.cookie, { id: randomUUID(), decision: 'approve' });
  assert.equal(jobState(original).run_cycle, 1);
  assert.equal(remote.calls.filter(c => c.name === 'update_order').length, 1);
  await review.getByRole('button', { name: 'Close', exact: true }).click();
  // A saved continuation reads like the agent typing again, not a system label.
  await page.locator('.chat-reply-progress [role=status]').filter({ hasText: 'is typing…' }).waitFor();
  await page.screenshot({ path: join(app.data, 'queued-continuation.png') });

  await app.stopApi(); await app.startApi();
  const replacement = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  const final = await app.done(original);
  assert.match(final.content, /CONTINUED_RESULT/);
  assert.match(final.content, /APPROVED_FOR_ORIGINAL_TASK/);
  assert.equal(jobState(original).payload.prompt, original.prompt);
  assert.equal(jobState(original).run_cycle, 1);
  assert.equal(remote.calls.filter(c => c.name === 'update_order').length, 1);
  assert.equal(remote.calls.filter(c => c.name === 'read_order').length, 1);
  const owners = JSON.parse(app.sql(`SELECT json_agg(c.executor ORDER BY a.generation) FROM relay_worker_assignments a JOIN relay_assignment_credentials c ON c.assignment=a.id WHERE a.reply='${original.reply}'`));
  assert.equal(owners.length, 2); assert.notEqual(owners[0], owners[1]);
  assert.equal((await snapshot()).actions.filter(a => a.job === original.reply).length, 2);
  await page.reload(); await page.getByText(/CONTINUED_RESULT/).waitFor();
  await page.screenshot({ path: join(app.data, 'continued-task.png') });
  app.pass('Approval and queued continuation survive two controller restarts; a different executor continues the original task, replays its write receipt and performs the remaining read once');

  const changes = await app.send(command('NEEDS_A_NEW_PROPOSAL'));
  const changeAction = await waiting(changes);
  await app.request(`${control}/actions/${changeAction.id}/decision`, app.cookie, { id: randomUUID(), decision: 'request_changes' });
  const changed = await app.done(changes);
  assert.match(changed.content, /request_changes/);
  assert.equal(remote.calls.filter(c => c.name === 'update_order').length, 1);

  const stopped = await app.send(command('STOP_BEFORE_APPROVAL'));
  const stoppedAction = await waiting(stopped);
  await page.reload();
  await cancelRequest(page);
  await app.done(stopped, 'cancelled');
  await app.request(`${control}/actions/${stoppedAction.id}/decision`, app.cookie, { id: randomUUID(), decision: 'approve' }, 400);
  assert.equal(jobState(stopped).state, 'finished');
  assert.equal((await actionFor(stopped)).state, 'invalidated');
  app.pass('Request changes continues with the saved decision and no write; Stop cancels a waiting task and invalidates its pending action');

  const uncertain = await app.send(command('APPLIED_BUT_UNCONFIRMED', 'lost'));
  const unknownAction = await waiting(uncertain);
  await app.request(`${control}/actions/${unknownAction.id}/decision`, app.cookie, { id: randomUUID(), decision: 'approve' });
  assert.equal((await actionFor(uncertain)).state, 'uncertain');
  assert.equal(jobState(uncertain).state, 'waiting');
  assert.equal(remote.records.get('lost').address, 'APPLIED_BUT_UNCONFIRMED');
  await app.stopApi(); await app.startApi(); await page.reload();
  await page.getByRole('status').filter({ hasText: 'Needs attention · External outcome not yet confirmed' }).waitFor();
  await page.evaluate(() => { document.documentElement.dataset.theme = 'dark'; });
  await page.screenshot({ path: join(app.data, 'uncertain-wait-dark.png') });
  const modelCount = app.provider.requests.length;
  await new Promise(resolve => setTimeout(resolve, 1200));
  assert.equal(app.provider.requests.length, modelCount);
  await app.request(`${control}/actions/${unknownAction.id}/decision`, app.cookie, { id: randomUUID(), decision: 'approve' });
  assert.equal(remote.calls.filter(c => c.name === 'update_order' && c.arguments.order === 'lost').length, 1);
  await cancelRequest(page);
  await app.done(uncertain, 'cancelled');
  assert.equal((await actionFor(uncertain)).state, 'uncertain');
  assert.deepEqual(errors, []);
  assert.equal(JSON.stringify(app.provider.requests).includes(remote.token), false);
  assert.equal(app.processes().some(p => p.logs.includes(remote.token)), false);
  await app.stop(replacement);
  app.pass('An applied but unconfirmed external write holds continuation across restart without retry or model polling; Stop retains the uncertain receipt');
});
