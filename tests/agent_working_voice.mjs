// The agent's working voice with real Goose: it acknowledges long work through
// send_update while it keeps going, a request that queues behind that work hears
// "I'm on another request" at once with no model call, "typing" only shows while
// the agent is actually at work, and the final reply still carries the result.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { controllerToolsOnly } from './fixtures/worker_control.mjs';
import { until, root } from './fixtures/polling.mjs';
const { chromium } = createRequire(join(root, 'web/package.json'))('@playwright/test');

test('an agent says "got it" and keeps working, tells a second requester it is busy, then delivers', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 240000,
}, async t => {
  let browser; const errors = [];
  t.after(async () => { await browser?.close(); });
  const app = await workerFixture(t, { mode: 'pooled', control: true, capacity: 1, workspaceLimit: 1, modelLimit: 1, leaseSeconds: 15 });
  const w = app.path.split('/').at(-1), member = await app.login(1);
  const person = (await app.request('/api/identity/session', member)).value.user;
  await app.request(`${app.path}/chat-grants`, app.cookie, { user: person.id, agent: app.profile.id, allowed: true });
  const channel = (await app.request(`${app.path}/chats`, app.cookie, { id: randomUUID(), kind: 'channel', name: 'Accounts', users: [person.id], agents: [app.profile.id] })).value.id;
  const said = reply => app.sql(`SELECT string_agg(content,' | ' ORDER BY sequence) FROM relay_chat_messages WHERE update_of='${reply}'`);
  const attempt = reply => app.sql(`SELECT a.state FROM relay_worker_assignments a JOIN relay_chat_jobs j ON a.id=j.claim WHERE j.reply='${reply}'`);

  const long = 'Reconcile September against the bank statement';
  app.provider.setControl(long, { steps: [{ tool: 'send_update', arguments: { text: "Got it. I'll match the 212 bank lines to invoices and list anything I can't place." } }],
    checkpoint: 'Done. 206 matched, 6 unmatched.' });
  // Hold the final reply mid-stream so the agent is genuinely at work.
  app.provider.setStream({ chunks: ['Done. ', '206 matched, 6 unmatched.'], holdAfter: 1 });
  app.provider.setOutputHeld(true);
  await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  const first = await app.send(long, channel, app.cookie);
  await until(() => said(first.reply)?.startsWith('Got it.'), 'The acknowledgement did not arrive', 35000);
  await until(() => attempt(first.reply) === 'running', 'The request is not running', 20000);
  assert.equal(app.sql(`SELECT state FROM relay_chat_messages WHERE id='${first.reply}'`), 'queued', 'the reply is still in progress after the acknowledgement');
  app.pass('Real Goose acknowledged long work through send_update and kept working; the reply stayed open');

  // A second person asks while it works: an instant answer, no model call. The
  // first request makes exactly two model calls (the tool call, then the reply).
  await until(() => app.provider.requests.length >= 2, 'The first request did not reach its reply', 20000);
  const calls = app.provider.requests.length;
  const second = await app.send("What's Sharma Traders' outstanding balance?", channel, member);
  assert.equal(said(second.reply), `I'm working on ${app.sql(`SELECT COALESCE(display_name,email) FROM relay_users WHERE id='${first.requester}'`)}'s request at the moment; I'll start on this right after.`);
  assert.equal(app.provider.requests.length, calls, 'the busy reply used no model call');
  assert.equal(app.sql(`SELECT state FROM relay_chat_jobs WHERE reply='${second.reply}'`), 'queued', 'the second request is only queued');

  browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1440, height: 960 } });
  const [name, value] = member.split('=');
  await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
  const page = await context.newPage(); page.on('pageerror', error => errors.push(error.message));
  await page.goto(`${app.base}/w/${w}/?view=chat&conversation=${channel}`);
  const log = page.getByRole('log', { name: 'Messages', exact: true });
  await log.getByText("Got it. I'll match the 212 bank lines", { exact: false }).waitFor();
  await log.getByText("I'm working on", { exact: false }).waitFor();
  await log.getByText('Waiting to start', { exact: true }).waitFor();
  await log.getByText('Nova is typing…', { exact: true }).waitFor();
  await page.screenshot({ path: join(app.data, 'working-voice-busy.png') });
  app.pass('The second requester saw the busy reply at once, their own request as "Waiting to start", and typing only on the request actually running');

  app.provider.setOutputHeld(false);
  await app.done(first);
  assert.match(app.sql(`SELECT content FROM relay_chat_messages WHERE id='${first.reply}'`), /206 matched, 6 unmatched/);
  await app.done(second);
  const details = (await app.request(`${app.path}/activity/requests/${first.reply}`, app.cookie)).value;
  assert.deepEqual(details.steps.map(step => step.title), ["Said: Got it. I'll match the 212 bank lines to invoices and list anything I can't place."]);
  controllerToolsOnly(app.provider.requests.at(-1));
  // The scripted model gives every later reply the same text; the first bubble is the request's.
  await log.getByText('206 matched, 6 unmatched', { exact: false }).first().waitFor();
  assert.equal(await log.getByText('Waiting to start', { exact: true }).count(), 0);
  assert.deepEqual(errors, []);
  app.pass('The reply carried the result, the queued request then ran to completion, and Activity shows what the agent said as a step');
});
