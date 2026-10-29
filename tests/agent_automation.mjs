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

test('shared schedules retain exact creation, release a real Goose question, separately approve one effect and never revive stopped or revoked work', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 240000,
}, async t => {
  let app, browser; const pages = [], errors = [];
  t.after(async () => {
    try { if (app && !t.passed) for (const [i, page] of pages.entries()) {
      await page.screenshot({ path: join(app.data, `automation-failure-${i}.png`), timeout: 5000 }).catch(() => {});
      await writeFile(join(app.data, `automation-failure-${i}.html`), await page.content()).catch(() => {});
    } } finally { await browser?.close(); }
  });
  const remote = await remoteControlMcp(); t.after(() => remote.close());
  const directory = await mkdtemp(join(root, '.tmp/schedule-profile-'));
  const profiles = join(directory, 'servers.json'), bearer = join(directory, 'bearer');
  await writeFile(profiles, '[]', { mode: 0o600 }); await writeFile(bearer, remote.token, { mode: 0o600 });
  app = await workerFixture(t, { mode: 'pooled', control: true, capacity: 1, workspaceLimit: 1, modelLimit: 1, leaseSeconds: 15, apiEnv: { AYATI_MCP_SERVERS_FILE: profiles } });
  const w = app.path.split('/').at(-1), member = await app.login(1), owner = (await app.request('/api/identity/session', app.cookie)).value.user;
  const person = (await app.request('/api/identity/session', member)).value.user;
  await app.request(`${app.path}/chat-grants`, app.cookie, { user: person.id, agent: app.profile.id, allowed: true });
  const channel = (await app.request(`${app.path}/chats`, app.cookie, { id: randomUUID(), kind: 'channel', name: 'Scheduled delivery operations', users: [person.id], agents: [app.profile.id] })).value.id;
  const path = `${app.path}/agent-profiles/${app.profile.id}/schedules`;
  const row = id => JSON.parse(app.sql(`SELECT to_jsonb(s) FROM relay_agent_schedules s WHERE id='${id}'`));
  const runs = id => JSON.parse(app.sql(`SELECT coalesce(json_agg(o ORDER BY due_ms DESC),'[]'::json) FROM relay_schedule_occurrences o WHERE schedule='${id}'`));
  const state = reply => JSON.parse(app.sql(`SELECT row_to_json(j) FROM relay_chat_jobs j WHERE reply='${reply}'`));
  browser = await chromium.launch({ headless: true });
  async function pageFor(cookie) {
    const context = await browser.newContext({ viewport: { width: 1440, height: 960 } });
    const [name, value] = cookie.split('='); await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
    const page = await context.newPage(); page.on('pageerror', error => errors.push(error.message)); pages.push(page); return page;
  }
  const page = await pageFor(app.cookie), staff = await pageFor(member), agents = `${app.base}/w/${w}/?view=agents`;
  await page.goto(agents); await page.getByRole('tab', { name: 'Automation', exact: true }).click();
  await page.getByRole('button', { name: 'Create schedule', exact: true }).click();
  let dialog = page.getByRole('dialog', { name: 'Create schedule', exact: true });
  await dialog.getByRole('textbox', { name: 'Routine name', exact: true }).fill('Daily delivery review');
  await dialog.getByRole('textbox', { name: 'Task instructions', exact: true }).fill('Prepare the daily delivery summary without changing any order.');
  await dialog.getByLabel('Destination conversation', { exact: true }).selectOption(channel);
  await dialog.getByLabel('Repeat', { exact: true }).selectOption('interval');
  await dialog.getByLabel('Unit', { exact: true }).selectOption('3600');
  await dialog.getByRole('button', { name: 'Review schedule', exact: true }).click();
  await dialog.getByText('Scheduled delivery operations', { exact: true }).waitFor();
  assert.equal(app.provider.requests.length, 0); assert.equal(app.sql('SELECT count(*) FROM relay_chat_jobs'), '0');
  await page.screenshot({ path: join(app.data, 'schedule-review-desktop.png') });
  let discardAcknowledgment = true; const creationBodies = [];
  await page.route(`**${path}`, async route => {
    if (route.request().method() !== 'POST') return route.continue();
    creationBodies.push(route.request().postDataJSON());
    if (!discardAcknowledgment) return route.continue();
    discardAcknowledgment = false; const accepted = await route.fetch(); assert.equal(accepted.status(), 200); await route.abort('failed');
  });
  await dialog.getByRole('button', { name: 'Create schedule', exact: true }).click();
  await dialog.getByRole('button', { name: 'Edit as a new schedule', exact: true }).waitFor();
  assert.equal(app.sql('SELECT count(*) FROM relay_agent_schedules'), '1');
  await page.reload(); await page.getByRole('tab', { name: 'Automation', exact: true }).click();
  await page.getByRole('button', { name: 'Create schedule', exact: true }).click();
  dialog = page.getByRole('dialog', { name: 'Create schedule', exact: true });
  await dialog.getByRole('button', { name: 'Retry same save', exact: true }).click();
  await dialog.getByText('Schedule saved.', { exact: false }).waitFor();
  assert.deepEqual(creationBodies[1], creationBodies[0]); assert.equal(app.sql('SELECT count(*) FROM relay_agent_schedules'), '1');
  await dialog.getByRole('button', { name: 'Done', exact: true }).click();
  await page.unroute(`**${path}`);
  const planned = creationBodies[0].id;
  await page.getByRole('article', { name: 'Routine: Daily delivery review', exact: true }).getByRole('button', { name: 'Pause', exact: true }).click();
  let pause = page.getByRole('dialog', { name: 'Pause routine', exact: true }); await pause.getByRole('button', { name: 'Pause routine', exact: true }).click(); await pause.getByRole('button', { name: 'Done', exact: true }).click();
  assert.equal(row(planned).state, 'paused'); assert.equal(app.provider.requests.length, 0);
  await staff.goto(agents); await staff.getByRole('tab', { name: 'Automation', exact: true }).click();
  await staff.getByText("Only this agent's maintainers can manage its shared routines.", { exact: false }).waitFor(); assert.equal(await staff.getByRole('button', { name: 'Create schedule', exact: true }).count(), 0);
  app.pass('The schedule review starts no model work; a dropped real creation acknowledgment survives browser reload and retries the exact saved payload once; staff cannot manage a maintainer routine');

  const connection = randomUUID(), control = `${app.path}/agent-profiles/${app.profile.id}/control`;
  await writeFile(profiles, JSON.stringify([{ id: 'orders', workspace: w, name: 'Company orders', account: 'Test company', url: remote.url, bearer_file: bearer, operations: remote.operations }]));
  await app.request(`${app.path}/control/systems`, app.cookie, { server: 'orders', approved: true, revision: 0 });
  await app.request(`${app.path}/control/connections`, app.cookie, { id: connection, server: 'orders', enabled: true, revision: 0 });
  for (const [operation, mode] of [['order.read', 'allow'], ['order.update', 'ask']]) await app.request(`${control}/rules`, app.cookie, { connection, operation, mode, revision: 0, chat: channel });
  const prompt = 'Ask for delivery method then prepare the separately approved scheduled update';
  const specification = { version: 1, kind: 'single_choice', title: 'Scheduled delivery method', options: [{ id: 'road', label: 'Road' }, { id: 'air', label: 'Air' }] };
  const write = { tool: 'request_action', arguments: { connection, operation: 'order.update', arguments: { order: 'one', address: 'ONE_APPROVED_SCHEDULED_UPDATE' } } };
  app.provider.setControl(prompt, { steps: [{ tool: 'ask_question', arguments: { key: 'delivery', specification } }], checkpoint: 'I saved a delivery question; no order changed.',
    afterQuestion: [{ tool: 'operations', arguments: {} }, write], questionCheckpoint: 'Your answer is saved. The exact update needs separate approval.',
    after: [{ tool: 'operations', arguments: {} }, write, { tool: 'request_action', arguments: { connection, operation: 'order.read', arguments: { order: 'one' } } }] });
  let executor = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  const live = { id: randomUUID(), chat: channel, name: 'Reviewed scheduled update', content: prompt, first_ms: Date.now() + 3000, interval_seconds: 10, timezone: 'Asia/Kolkata' };
  await app.request(path, app.cookie, live);
  await until(() => runs(live.id).length === 1, 'Controller did not admit the due occurrence');
  const occurrence = runs(live.id)[0], job = JSON.parse(app.sql(`SELECT payload FROM relay_chat_jobs WHERE reply='${occurrence.reply}'`));
  assert.equal(job.requester, owner.id); assert.equal(job.chat, channel);
  await until(() => state(job.reply).state === 'waiting', 'Scheduled question did not release execution', 35000);
  const waitingCalls = app.provider.requests.length; assert.equal(remote.calls.length, 0);
  await page.getByRole('button', { name: 'Refresh schedules', exact: true }).click();
  let routine = page.getByRole('article', { name: `Routine: ${live.name}`, exact: true });
  await routine.getByRole('button', { name: 'View runs', exact: true }).click();
  let runDialog = page.getByRole('dialog', { name: `${live.name} · Runs`, exact: true });
  await runDialog.getByText('Waiting for an answer', { exact: true }).waitFor();
  await page.screenshot({ path: join(app.data, 'schedule-runs-desktop.png') });
  await runDialog.getByRole('button', { name: 'Close', exact: true }).click();
  await routine.getByRole('button', { name: 'Pause', exact: true }).click(); pause = page.getByRole('dialog', { name: 'Pause routine', exact: true });
  await pause.getByRole('button', { name: 'Pause routine', exact: true }).click(); await pause.getByRole('button', { name: 'Done', exact: true }).click();
  assert.equal(state(job.reply).state, 'waiting'); assert.equal(app.provider.requests.length, waitingCalls);
  await routine.getByRole('button', { name: 'View runs', exact: true }).click(); runDialog = page.getByRole('dialog', { name: `${live.name} · Runs`, exact: true });
  await runDialog.getByRole('link', { name: 'Open run in Chat', exact: true }).click();
  await page.waitForURL(new RegExp(`conversation=${channel}&message=${job.reply}`));
  await page.getByRole('button', { name: 'Answer question', exact: true }).click();
  const question = page.getByRole('dialog', { name: 'Scheduled delivery method', exact: true }); await question.getByLabel('Road', { exact: true }).check();
  await question.getByRole('button', { name: 'Submit answer', exact: true }).click(); await question.getByText('Answer saved', { exact: true }).waitFor(); await question.getByRole('button', { name: 'Close', exact: true }).click();
  await until(() => state(job.reply).state === 'waiting' && app.sql(`SELECT count(*) FROM relay_control_actions WHERE job='${job.reply}' AND state='pending'`) === '1', 'Schedule answer did not reach an independent approval', 35000);
  assert.equal(remote.calls.length, 0);
  await page.getByRole('button', { name: 'Review saved request', exact: true }).click();
  const approval = page.getByRole('dialog', { name: 'Review approval request', exact: true }); await approval.getByText('ONE_APPROVED_SCHEDULED_UPDATE', { exact: true }).waitFor();
  await approval.getByRole('button', { name: 'Approve this action', exact: true }).click(); await approval.getByRole('button', { name: 'Close', exact: true }).click(); await app.done(job);
  assert.equal(remote.calls.filter(call => call.name === 'update_order').length, 1); assert.equal(remote.records.get('one').address, 'ONE_APPROVED_SCHEDULED_UPDATE'); assert.equal(row(live.id).state, 'paused');
  app.pass('A real scheduled Goose task retains the original human/channel, asks an ordinary question, releases execution and continues after a separate exact action approval; pausing its routine preserves that saved task and exactly one confirmed write');

  await app.stop(executor); await app.stopApi(); await app.startApi();
  const finishedCalls = app.provider.requests.length;
  await page.goto(agents); await page.getByRole('tab', { name: 'Automation', exact: true }).click();
  routine = page.getByRole('article', { name: `Routine: ${live.name}`, exact: true }); await routine.getByText('paused', { exact: true }).waitFor();
  await page.setViewportSize({ width: 390, height: 844 }); await page.emulateMedia({ colorScheme: 'dark' });
  await page.getByRole('button', { name: 'Open agent Nova', exact: true }).click();
  await routine.getByRole('button', { name: 'Review and resume', exact: true }).click();
  let resume = page.getByRole('dialog', { name: 'Resume routine', exact: true }); await resume.getByText(live.content, { exact: true }).waitFor(); await page.screenshot({ path: join(app.data, 'schedule-resume-mobile-dark.png') });
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false); await page.setViewportSize({ width: 320, height: 600 }); assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false); assert.equal(await resume.evaluate(node => { const bounds = node.getBoundingClientRect(); return bounds.top >= 0 && bounds.bottom <= innerHeight; }), true);
  await page.keyboard.press('Escape'); await resume.waitFor({ state: 'detached' }); assert.equal(app.provider.requests.length, finishedCalls); assert.equal(runs(live.id).length, 1);
  await page.setViewportSize({ width: 1440, height: 960 }); await page.emulateMedia({ colorScheme: 'light' });
  await routine.getByRole('button', { name: 'Review and resume', exact: true }).click(); resume = page.getByRole('dialog', { name: 'Resume routine', exact: true });
  await resume.getByRole('button', { name: 'Resume routine', exact: true }).click(); await resume.getByRole('button', { name: 'Done', exact: true }).click();
  await until(() => runs(live.id).length === 2, 'Explicit resume did not admit one latest distinct occurrence', 20000);
  const next = runs(live.id)[0]; assert.notEqual(next.reply, occurrence.reply); assert.equal(app.provider.requests.length, finishedCalls);
  await app.request(`${app.path}/chats/${channel}/messages/${next.reply}/cancel`, app.cookie, {});
  assert.equal(state(next.reply).state, 'finished'); assert.equal(row(live.id).state, 'enabled');
  await app.request(`${path}/${live.id}/state`, app.cookie, { request: randomUUID(), revision: row(live.id).revision, enabled: false });
  await app.stopApi(); await app.startApi(); assert.equal(state(next.reply).state, 'finished'); assert.equal(runs(live.id).length, 2);
  assert.equal(remote.calls.filter(call => call.name === 'update_order').length, 1);
  app.pass('Controller restart and status/review reads do not rerun completed work; explicit resume creates one distinct latest occurrence; Stop terminates that run without disabling the routine, then an explicit pause survives another restart');

  await app.request(`${app.path}/members/${person.id}`, app.cookie, { role: 'Owner' });
  const blocked = { id: randomUUID(), chat: channel, name: 'PRIVATE_REVOKED_ROUTINE', content: 'PRIVATE_REVOKED_TASK', first_ms: Date.now() + 60000, interval_seconds: 10, timezone: 'Asia/Kolkata' };
  await app.request(path, app.cookie, blocked);
  await page.goto(agents); await page.getByRole('tab', { name: 'Automation', exact: true }).click(); await page.getByRole('article', { name: 'Routine: PRIVATE_REVOKED_ROUTINE', exact: true }).waitFor();
  await app.request(`${app.path}/chats/${channel}/members`, member, { users: [person.id] });
  await until(() => page.getByText('PRIVATE_REVOKED_ROUTINE', { exact: true }).count().then(count => count === 0), 'Revoked source details remained visible');
  assert.equal(row(blocked.id).state, 'blocked'); assert.equal((await app.request(path, app.cookie)).value.schedules.length, 0);
  await app.request(`${app.path}/chats/${channel}/members`, member, { users: [person.id, owner.id] });
  const denied = await app.request(`${path}/${blocked.id}/state`, member, { request: randomUUID(), revision: row(blocked.id).revision, enabled: true }, 400); assert.match(denied.value.error, /original authorizing person/);
  await page.getByRole('button', { name: 'Refresh schedules', exact: true }).click(); await page.getByRole('article', { name: 'Routine: PRIVATE_REVOKED_ROUTINE', exact: true }).getByText('blocked', { exact: true }).waitFor();
  assert.equal(runs(blocked.id).length, 0); assert.equal(remote.calls.filter(call => call.name === 'update_order').length, 1); assert.deepEqual(errors, []);
  await page.screenshot({ path: join(app.data, 'schedules-blocked-desktop.png') });
  await writeFile(join(root, '.tmp/shared-agent-automation/browser-evidence.json'), JSON.stringify({ data: app.data, schedule: live.id, channel, reply: job.reply, occurrences: runs(live.id).length, model_requests: app.provider.requests.length, confirmed_writes: remote.calls.filter(call => call.name === 'update_order').length, errors }, null, 2));
});
