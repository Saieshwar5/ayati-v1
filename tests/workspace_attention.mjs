import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { mkdtemp, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { remoteControlMcp } from './fixtures/remote_control_mcp.mjs';
import { until, root } from './fixtures/polling.mjs';
const { chromium } = createRequire(join(root, 'web/package.json'))('@playwright/test');

test('Activity Needs me retains personal reads, resumes exact threaded work, reviews failed outcomes and clears revoked sources', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 240000,
}, async t => {
  let app, browser; const pages = [], errors = [];
  t.after(async () => {
    try {
      if (app && !t.passed) for (const [index, page] of pages.entries()) {
        await page.screenshot({ path: join(app.data, `attention-failure-${index}.png`), timeout: 5000 }).catch(() => {});
        await writeFile(join(app.data, `attention-failure-${index}.html`), await page.content()).catch(() => {});
      }
    } finally { await browser?.close(); }
  });
  const remote = await remoteControlMcp(); t.after(() => remote.close());
  const directory = await mkdtemp(join(root, '.tmp/attention-profile-'));
  const profiles = join(directory, 'servers.json'), bearer = join(directory, 'bearer');
  await writeFile(profiles, '[]', { mode: 0o600 }); await writeFile(bearer, remote.token, { mode: 0o600 });
  app = await workerFixture(t, { mode: 'pooled', control: true, capacity: 1, workspaceLimit: 1, modelLimit: 1,
    leaseSeconds: 15, apiEnv: { AYATI_MCP_SERVERS_FILE: profiles } });
  const w = app.path.split('/').at(-1), member = await app.login(1);
  const owner = (await app.request('/api/identity/session', app.cookie)).value.user;
  const person = (await app.request('/api/identity/session', member)).value.user;
  await app.request(`${app.path}/chat-grants`, app.cookie, { user: person.id, agent: app.profile.id, allowed: true });
  const channel = (await app.request(`${app.path}/chats`, app.cookie, { id: randomUUID(), kind: 'channel', name: 'Delivery coordination', users: [person.id], agents: [app.profile.id] })).value.id;
  const thread = randomUUID(); await app.request(`${app.path}/chats/${channel}/messages`, app.cookie, { id: thread, content: 'Prepare a reviewed delivery update.' });
  const connection = randomUUID(), control = `${app.path}/agent-profiles/${app.profile.id}/control`;
  await writeFile(profiles, JSON.stringify([{ id: 'orders', workspace: w, name: 'Company orders', account: 'Test company', url: remote.url, bearer_file: bearer, operations: remote.operations }]));
  await app.request(`${app.path}/control/systems`, app.cookie, { server: 'orders', approved: true, revision: 0 });
  await app.request(`${app.path}/control/connections`, app.cookie, { id: connection, server: 'orders', enabled: true, revision: 0 });
  // The admin's rules for this connector, then the agent's Tools switch (schema 47).
  for (const [operation, mode] of [['order.read', 'allow'], ['order.update', 'ask']]) await app.request(`${app.path}/control/connector-rules`, app.cookie, { connection, operation, mode, needs_other: false, revision: 0 });
  await app.request(`${control}/tools`, app.cookie, { connection, enabled: true });
  const prompt = 'Ask for delivery method and prepare the separate exact reviewed update';
  const specification = { version: 1, kind: 'single_choice', title: 'Choose delivery method', options: [{ id: 'road', label: 'Road' }, { id: 'air', label: 'Air' }] };
  const write = { tool: 'request_action', arguments: { connection, operation: 'order.update', arguments: { order: 'one', address: 'REVIEWED_FROM_HOME_ATTENTION' } } };
  app.provider.setControl(prompt, { steps: [{ tool: 'ask_question', arguments: { key: 'delivery', specification } }], checkpoint: 'I saved your delivery question and will continue after an answer.',
    afterQuestion: [{ tool: 'operations', arguments: {} }, write], questionCheckpoint: 'Your answer is saved. The exact order update needs separate approval.',
    after: [{ tool: 'operations', arguments: {} }, write, { tool: 'request_action', arguments: { connection, operation: 'order.read', arguments: { order: 'one' } } }] });
  let executor = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  await app.request(`${app.path}/chats/${channel}/messages`, app.cookie, { id: randomUUID(), content: prompt, agent: app.profile.id, thread });
  const job = JSON.parse(app.sql(`SELECT j.payload FROM relay_chat_jobs j JOIN relay_chat_messages m ON m.id=j.reply WHERE m.thread='${thread}' ORDER BY j.sequence DESC LIMIT 1`));
  const state = reply => JSON.parse(app.sql(`SELECT row_to_json(j) FROM relay_chat_jobs j WHERE reply='${reply}'`));
  await until(() => state(job.reply).state === 'waiting', 'Question did not release execution', 35000);
  const idleCalls = app.provider.requests.length; assert.equal(remote.calls.length, 0);
  browser = await chromium.launch({ headless: true });
  async function pageFor(cookie) {
    const context = await browser.newContext({ viewport: { width: 1440, height: 960 } });
    const [name, value] = cookie.split('='); await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
    const page = await context.newPage(); page.on('pageerror', error => errors.push(error.message)); pages.push(page); return page;
  }
  const page = await pageFor(app.cookie), otherSession = await pageFor(app.cookie), observer = await pageFor(member);
  const home = `${app.base}/w/${w}/?view=home`, activity = `${app.base}/w/${w}/?view=activity`;
  const inboxFor = page => page.getByRole('region', { name: 'Needs me', exact: true });
  const unread = inbox => inbox.getByText(/· new$/).count();
  // Home's small panel opens Activity → Needs me.
  await page.goto(home); await observer.goto(activity);
  await page.getByRole('link', { name: 'Open Activity · 1', exact: true }).click(); await page.waitForURL(/view=activity&tab=needs/);
  await otherSession.goto(activity);
  let inbox = inboxFor(page); await inbox.getByText('Choose delivery method', { exact: true }).waitFor();
  await page.getByRole('tab', { name: /^Needs me/ }).getByText('1', { exact: true }).waitFor();
  assert.equal(await observer.getByText('Choose delivery method', { exact: true }).count(), 0);
  await page.screenshot({ path: join(app.data, 'attention-inbox-desktop.png') });
  const answer = inbox.getByRole('button', { name: 'Answer', exact: true }); await answer.focus(); await page.keyboard.press('Enter');
  let question = page.getByRole('dialog', { name: 'Choose delivery method', exact: true }); await question.waitFor();
  await page.keyboard.press('Escape'); await question.waitFor({ state: 'detached' });
  assert.equal(await answer.evaluate(node => node === document.activeElement), true);
  await until(() => unread(inbox).then(count => count === 0), 'Reading did not save its personal marker');
  assert.equal(state(job.reply).state, 'waiting'); assert.equal(remote.calls.length, 0); assert.equal(app.provider.requests.length, idleCalls);
  await otherSession.reload(); await inboxFor(otherSession).getByText('Choose delivery method', { exact: true }).waitFor();
  assert.equal(await unread(inboxFor(otherSession)), 0);
  await app.stop(executor); await app.stopApi(); await app.startApi(); await page.reload(); inbox = inboxFor(page);
  await inbox.getByText('Choose delivery method', { exact: true }).waitFor();
  assert.equal(await unread(inbox), 0);
  app.pass('Needs me derives a personal question queue, keyboard review saves only read state, a second session/restart retains it, and another channel member receives no requester notification');
  await inbox.getByRole('button', { name: 'Answer', exact: true }).click(); question = page.getByRole('dialog', { name: 'Choose delivery method', exact: true });
  await question.getByLabel('Road', { exact: true }).check(); await question.getByRole('button', { name: 'Submit answer', exact: true }).click();
  await question.getByText('Answer saved', { exact: true }).waitFor(); assert.equal(remote.calls.length, 0);
  await question.getByRole('button', { name: 'Close', exact: true }).click();
  executor = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  await until(() => state(job.reply).state === 'waiting' && app.sql(`SELECT count(*) FROM relay_control_actions WHERE job='${job.reply}' AND state='pending'`) === '1', 'Answer did not reach separate approval', 35000);
  await inbox.getByRole('button', { name: 'Review', exact: true }).click();
  const approval = page.getByRole('dialog', { name: 'Review approval request', exact: true });
  await approval.getByText('REVIEWED_FROM_HOME_ATTENTION', { exact: true }).waitFor(); assert.equal(remote.calls.length, 0);
  await approval.getByRole('button', { name: 'Approve this action', exact: true }).click();
  await approval.getByRole('button', { name: 'Close', exact: true }).click(); await app.done(job);
  assert.equal(remote.calls.filter(call => call.name === 'update_order').length, 1);
  await inbox.getByText('Nothing needs you right now', { exact: true }).waitFor();
  // History shows the finished request, its approved step, and opens its source.
  await page.getByRole('tab', { name: 'History', exact: true }).click();
  const history = page.getByRole('region', { name: 'History', exact: true });
  await history.getByRole('button', { name: new RegExp(prompt) }).click();
  const details = page.getByRole('dialog', { name: prompt, exact: true });
  await details.getByText(/Approved by/).first().waitFor();
  await details.getByRole('link', { name: 'Open conversation', exact: true }).click();
  await page.waitForURL(new RegExp(`conversation=${channel}&message=${job.reply}&thread=${thread}`));
  await page.getByRole('dialog', { name: 'Thread', exact: true }).waitFor();
  app.pass('Needs me question and exact approval reviews continue the original real Goose task/thread and confirm one authenticated MCP write; History shows the approved step and opens its saved source');

  app.provider.setUnavailable(true);
  const failed = await app.send('Prepare a failure outcome for explicit review'); await app.done(failed, 'failed'); app.provider.setUnavailable(false);
  const failedState = state(failed.reply), afterFailureCalls = app.provider.requests.length;
  await page.goto(activity); inbox = inboxFor(page);
  await inbox.getByRole('button', { name: 'Mark reviewed', exact: true }).click();
  await inbox.getByText('Nothing needs you right now', { exact: true }).waitFor();
  assert.equal(app.provider.requests.length, afterFailureCalls); assert.equal(state(failed.reply).state, failedState.state);
  await page.getByRole('tab', { name: 'History', exact: true }).click();
  await page.getByRole('region', { name: 'History', exact: true }).locator('.activity-row').filter({ hasText: 'Could not complete' }).first().waitFor();
  await page.setViewportSize({ width: 390, height: 844 }); await page.emulateMedia({ colorScheme: 'dark' });
  await page.screenshot({ path: join(app.data, 'attention-recent-mobile-dark.png') });
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
  await page.setViewportSize({ width: 1440, height: 960 }); await page.emulateMedia({ colorScheme: 'light' });
  const privatePrompt = 'Ask the current requester for one missing private fact';
  app.provider.setControl(privatePrompt, { steps: [{ tool: 'ask_question', arguments: { key: 'private-fact', specification: { ...specification, title: 'PRIVATE_MEMBER_QUESTION' } } }], checkpoint: 'I saved a private requester question.' });
  const privateJob = await app.send(privatePrompt, channel, member); await until(() => state(privateJob.reply).state === 'waiting', 'Member question did not wait', 35000);
  await observer.reload();
  await inboxFor(observer).getByText('PRIVATE_MEMBER_QUESTION', { exact: true }).waitFor();
  assert.equal((await app.request(`${app.path}/attention`, app.cookie)).value.items.some(item => item.reply === privateJob.reply), false);
  await app.request(`${app.path}/chats/${channel}/members`, app.cookie, { users: [owner.id] });
  await until(() => observer.getByText('PRIVATE_MEMBER_QUESTION', { exact: true }).count().then(count => count === 0), 'Revoked question remained in Home attention');
  assert.deepEqual((await app.request(`${app.path}/attention`, member)).value.items, []);
  const finalCalls = app.provider.requests.length;
  await app.stop(executor); await app.stopApi(); await app.startApi(); await observer.reload();
  assert.equal(await observer.getByText('PRIVATE_MEMBER_QUESTION', { exact: true }).count(), 0); assert.equal(app.provider.requests.length, finalCalls);
  assert.equal(app.sql(`SELECT state FROM relay_task_questions WHERE reply='${privateJob.reply}'`), 'invalidated');
  assert.deepEqual(errors, []);
  app.pass('Failed-outcome review creates no retry or state change; mobile/dark viewing stays bounded; source revocation removes a requester notice and restart does not revive it');
  await writeFile(join(root, '.tmp/workspace-attention/browser-evidence.json'), JSON.stringify({ data: app.data, channel, reply: job.reply, failed: failed.reply,
    model_requests: app.provider.requests.length, confirmed_writes: remote.calls.filter(call => call.name === 'update_order').length, errors }, null, 2));
});
