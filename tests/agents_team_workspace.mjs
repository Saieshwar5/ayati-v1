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

test('selected Agents workspace exposes actual private work, exact Stop and original channel threads', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 240000,
}, async t => {
  const remote = await remoteControlMcp(); t.after(() => remote.close());
  let app, browser; const pages = [], errors = [];
  // Capture and close the browser before fixture shutdown, including failed assertions.
  t.after(async () => {
    try {
      if (app && !t.passed) for (const [i, p] of pages.entries()) {
        await p.screenshot({ path: join(app.data, `failure-${i}.png`), timeout: 5000 }).catch(() => {});
        await writeFile(join(app.data, `failure-${i}.html`), await p.content()).catch(() => {});
      }
      if (app) await writeFile(join(app.data, 'model-requests.json'), JSON.stringify(app.provider.requests, null, 2), { mode: 0o600 });
    } finally { await browser?.close(); }
  });
  const directory = await mkdtemp(join(root, '.tmp/agents-profile-'));
  const profiles = join(directory, 'servers.json'), bearer = join(directory, 'bearer');
  await writeFile(profiles, '[]', { mode: 0o600 }); await writeFile(bearer, remote.token, { mode: 0o600 });
  app = await workerFixture(t, { mode: 'pooled', control: true, runSeconds: 90, leaseSeconds: 15,
    apiEnv: { AYATI_MCP_SERVERS_FILE: profiles } });
  const w = app.path.split('/').at(-1), connection = randomUUID(), member = await app.login(1);
  const ownerUser = (await app.request('/api/identity/session', app.cookie)).value.user;
  const memberUser = (await app.request('/api/identity/session', member)).value.user;
  const team = [
    ['Meera', 'Coordinate suppliers, compare delivery options and follow up on open orders.'],
    ['Arjun', 'Help customers resolve support questions.'], ['Tara', 'Coordinate sales follow-ups.'],
    ['Nila', 'Prepare finance reports and check figures.'], ['Dev', 'Research products and suppliers.'],
    ['Kavya', 'Coordinate marketing work.'],
  ];
  const agents = [];
  for (const [name, role] of team) {
    const input = { id: randomUUID(), name, role, model: 'fixture', scope: 'workspace', revision: 0,
      settings: { provider: 'openai', escalation: 'When supplier terms or delivery details need a decision.' } };
    await app.request(`${app.path}/agent-profiles`, app.cookie, input); agents.push(input);
    await app.request(`${app.path}/chat-grants`, app.cookie, { user: memberUser.id, agent: input.id, allowed: true });
  }
  const agent = agents[0], control = `${app.path}/agent-profiles/${agent.id}/control`;
  await writeFile(profiles, JSON.stringify([{ id: 'orders', workspace: w, name: 'Company orders',
    account: 'Test company', url: remote.url, bearer_file: bearer, operations: remote.operations }]));
  await app.request(`${app.path}/control/systems`, app.cookie, { server: 'orders', approved: true, revision: 0 });
  await app.request(`${app.path}/control/connections`, app.cookie, { id: connection, server: 'orders', enabled: true, revision: 0 });
  const channel = (await app.request(`${app.path}/chats`, app.cookie, { id: randomUUID(), kind: 'channel', name: 'Supplier coordination', users: [memberUser.id], agents: [agent.id] })).value.id;
  for (const [operation, mode] of [['order.read', 'allow'], ['order.update', 'ask']]) {
    await app.request(`${control}/rules`, app.cookie, { connection, operation, mode, revision: 0, chat: channel });
  }
  const thread = randomUUID();
  await app.request(`${app.path}/chats/${channel}/messages`, member, { id: thread, content: 'Coordinate the delivery for order one.' });
  const taskText = 'Check order one and propose the reviewed delivery address.';
  app.provider.setControl(taskText, {
    steps: [{ tool: 'operations', arguments: {} },
      { tool: 'request_action', arguments: { connection, operation: 'order.read', arguments: { order: 'one' } } },
      { tool: 'request_action', arguments: { connection, operation: 'order.update', arguments: { order: 'one', address: 'Reviewed delivery address' } } }],
    checkpoint: 'I checked the order. The delivery update is saved for review; I will continue when the authorized person decides.',
  });
  const inputId = randomUUID();
  const worker = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  await app.request(`${app.path}/chats/${channel}/messages`, member, { id: inputId, content: taskText, agent: agent.id, thread });
  const job = JSON.parse(app.sql(`SELECT j.payload FROM relay_chat_jobs j JOIN relay_chat_messages m ON m.id=j.reply WHERE j.workspace='${w}' AND m.chat='${channel}' ORDER BY j.sequence DESC LIMIT 1`));
  const saved = async () => (await app.request(`${app.path}/agent-profiles/${agent.id}/requests/${job.reply}`, member)).value;
  await until(async () => (await saved()).state === 'approval', 'Human request did not reach its saved approval wait', 35000);
  assert.equal(remote.calls.filter(c => c.name === 'read_order').length, 1); assert.equal(remote.calls.filter(c => c.name === 'update_order').length, 0);
  assert.equal((await saved()).source_message, inputId); assert.equal((await saved()).thread, thread);

  browser = await chromium.launch({ headless: true });
  async function pageFor(cookie) {
    const context = await browser.newContext({ viewport: { width: 1440, height: 960 } });
    const [name, value] = cookie.split('='); await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
    const page = await context.newPage(); page.on('pageerror', error => errors.push(error.message)); pages.push(page); return page;
  }
  const page = await pageFor(member), ownerPage = await pageFor(app.cookie);
  const agentsUrl = `${app.base}/w/${w}/?view=agents`;
  await page.goto(agentsUrl); await page.getByRole('button', { name: 'Open agent Meera', exact: true }).click();
  await page.getByRole('heading', { name: 'My open requests', exact: true }).waitFor();
  await page.getByRole('article', { name: `Request: ${taskText}`, exact: true }).waitFor();
  const roster = await page.getByRole('complementary', { name: 'Agent team', exact: true }).boundingBox();
  const profile = await page.getByRole('main', { name: 'Selected agent', exact: true }).boundingBox();
  assert.ok(profile.x >= roster.x + roster.width - 1 && Math.abs(profile.y - roster.y) < 2, 'Desktop roster and profile must remain side by side');
  assert.equal(await page.getByRole('button', { name: '‹ Agents', exact: true }).isVisible(), false);
  await page.screenshot({ path: join(app.data, 'agents-overview-desktop.png') });
  assert.equal(await page.getByText(/meera@|Available around the clock/).count(), 0);
  await page.getByRole('tab', { name: 'Work', exact: true }).click();
  const modelCount = app.provider.requests.length;
  await page.getByRole('button', { name: 'Refresh progress', exact: true }).click();
  assert.equal(app.provider.requests.length, modelCount);
  app.pass('Selected B team/profile layout uses real responsibility, source-scoped work and account rules; progress refresh starts no model work');

  app.sql(`INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at) SELECT gen_random_uuid()::text,'${w}','${channel}','${memberUser.id}','user','Later ordinary conversation','sent',1 FROM generate_series(1,220)`);
  await page.getByRole('article', { name: `Request: ${taskText}`, exact: true }).getByRole('link', { name: 'Open conversation', exact: true }).click();
  const original = page.getByRole('dialog', { name: 'Thread', exact: true }); await original.waitFor();
  await original.locator(`[data-message-id="${job.reply}"]`).waitFor();
  assert.match(await original.innerText(), /delivery update is saved for review/);
  await page.screenshot({ path: join(app.data, 'original-thread-request.png') });
  await original.getByRole('button', { name: 'Close', exact: true }).click();
  await page.goto(agentsUrl); await page.getByRole('heading', { name: 'Meera', exact: true }).waitFor();
  await page.getByRole('tab', { name: 'Work', exact: true }).click();
  app.pass('Open conversation reaches the exact original request/thread beyond the latest history page; returning preserves the selected agent');

  const runningText = 'Prepare today’s supplier comparison and check the open commitments.';
  app.provider.setHeldMarkers([runningText]);
  const running = await app.send(runningText, channel, member, agent.id);
  await until(() => app.sql(`SELECT state FROM relay_worker_assignments WHERE id=(SELECT claim FROM relay_chat_jobs WHERE reply='${running.reply}')`) === 'running', 'Second task did not start while first waited', 35000);
  const queuedText = 'Prepare the next supplier follow-up.';
  const queued = await app.send(queuedText, channel, member, agent.id);
  const hidden = (await app.request(`${app.path}/chats`, app.cookie, { id: randomUUID(), name: 'Private finance', users: [], agents: [agent.id] })).value.id;
  const hiddenJob = await app.send('PRIVATE_FINANCE_REQUEST_MUST_NOT_LEAK', hidden, app.cookie, agent.id);
  await page.getByRole('button', { name: 'Refresh progress', exact: true }).click();
  await page.getByRole('article', { name: `Request: ${queuedText}`, exact: true }).waitFor();
  const visible = (await app.request(`${app.path}/agent-profiles/${agent.id}/requests?scope=visible`, member)).value;
  assert.equal(visible.requests.some(r => r.id === hiddenJob.reply), false); assert.equal(JSON.stringify(visible).includes('PRIVATE_FINANCE_REQUEST_MUST_NOT_LEAK'), false);
  assert.equal((await app.request(`${app.path}/agent-profiles`, member)).value.agents.find(a => a.id === agent.id).pending, 3);
  await page.screenshot({ path: join(app.data, 'agents-active-work-desktop.png') });
  await page.getByRole('article', { name: `Request: ${queuedText}`, exact: true }).getByRole('button', { name: 'Stop request', exact: true }).click();
  await until(() => app.sql(`SELECT state FROM relay_chat_messages WHERE id='${queued.reply}'`) === 'cancelled', 'Queued request was not cancelled');
  await page.getByRole('article', { name: `Request: ${runningText}`, exact: true }).getByRole('button', { name: 'Stop request', exact: true }).click();
  await until(() => app.sql(`SELECT state FROM relay_chat_messages WHERE id='${running.reply}'`) === 'cancelled', 'Running request was not cancelled');
  app.provider.setHeldMarkers([]);
  assert.notEqual(app.sql(`SELECT state FROM relay_chat_messages WHERE id='${hiddenJob.reply}'`), 'cancelled', 'Unrelated private request was stopped');
  assert.equal((await app.request(`${app.path}/agent-profiles`, member)).value.agents.find(a => a.id === agent.id).settings.paused, false);
  await page.getByRole('article', { name: `Request: ${taskText}`, exact: true }).getByRole('button', { name: 'Stop request', exact: true }).click();
  await until(async () => (await saved()).state === 'cancelled', 'Waiting request was not cancelled');
  const action = (await app.request(control, app.cookie)).value.actions.find(a => a.job === job.reply && a.operation === 'order.update');
  assert.equal(action.state, 'invalidated');
  await app.request(`${control}/actions/${action.id}/decision`, app.cookie, { id: randomUUID(), decision: 'approve' }, 400);
  assert.equal(remote.calls.filter(c => c.name === 'update_order').length, 0);
  app.pass('From Agents, requester stops queued/running/waiting work without pausing the agent or stopping private work; stale approval cannot write');

  await page.getByRole('tab', { name: 'Activity', exact: true }).click();
  await page.getByRole('article', { name: `Request: ${taskText}`, exact: true }).getByText(/Cancellation recorded by/).waitFor();
  await app.stop(worker);
  await app.stopApi(); await app.startApi(); await page.reload();
  await page.getByRole('tab', { name: 'Activity', exact: true }).click();
  await page.getByRole('article', { name: `Request: ${taskText}`, exact: true }).getByText(/Cancellation recorded by/).waitFor();
  await page.setViewportSize({ width: 390, height: 844 }); await page.getByRole('button', { name: '‹ Agents', exact: true }).click();
  await page.screenshot({ path: join(app.data, 'agents-roster-mobile.png') });
  await page.getByRole('button', { name: 'Open agent Meera', exact: true }).click();
  await page.getByRole('tab', { name: 'Overview', exact: true }).click();
  await page.getByText('No open requests from you.', { exact: false }).waitFor();
  await page.screenshot({ path: join(app.data, 'agents-profile-mobile.png') });
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
  await page.setViewportSize({ width: 1440, height: 700 }); await page.evaluate(() => { document.documentElement.dataset.theme = 'dark'; });
  await page.getByRole('tab', { name: 'Access', exact: true }).click();
  await page.screenshot({ path: join(app.data, 'agents-access-dark.png') });
  await ownerPage.goto(agentsUrl); await ownerPage.getByRole('button', { name: 'Open agent Meera', exact: true }).click();
  await ownerPage.getByRole('tab', { name: 'Work', exact: true }).click();
  await ownerPage.getByRole('combobox', { name: 'Show requests', exact: true }).selectOption('visible');
  assert.deepEqual(errors, []); assert.equal(JSON.stringify(app.provider.requests).includes(remote.token), false);
  assert.equal(app.processes().some(p => p.logs.includes(remote.token)), false);
  await app.stop(worker);
  app.pass('Cancellation history survives restart; desktop, phone roster/profile, short dark window and Owner-visible work remain usable without private leakage');
});
