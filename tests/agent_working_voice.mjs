// The agent's working voice with real Goose: it acknowledges long work through
// send_update while it keeps going; a request that queues behind that work is
// answered at once by the front desk (a short model turn in the agent's voice);
// "typing" only shows while the agent is actually at work; the final reply
// still carries the result.
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
  // Two slots: one for the long work, one for the front desk to answer at once.
  const app = await workerFixture(t, { mode: 'pooled', control: true, capacity: 2, workspaceLimit: 2, modelLimit: 2, leaseSeconds: 15 });
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
  // The front desk's own model turn, keyed on the new message.
  const question = "What's Sharma Traders' outstanding balance?";
  app.provider.setReply(`Front desk: ${question}`, '{"reply": "Sure, Vikram — I\'m in the middle of Ananya\'s September reconciliation; I\'ll pull Sharma\'s balance right after.", "answer_now": false}');
  await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '2' });
  const first = await app.send(long, channel, app.cookie);
  await until(() => said(first.reply)?.startsWith('Got it.'), 'The acknowledgement did not arrive', 35000);
  await until(() => attempt(first.reply) === 'running', 'The request is not running', 20000);
  assert.equal(app.sql(`SELECT state FROM relay_chat_messages WHERE id='${first.reply}'`), 'queued', 'the reply is still in progress after the acknowledgement');
  app.pass('Real Goose acknowledged long work through send_update and kept working; the reply stayed open');

  // A second person asks while it works: the front desk answers in the agent's
  // own words from a short model turn, while the work lane stays busy.
  await until(() => app.provider.requests.length >= 2, 'The first request did not reach its reply', 20000);
  const second = await app.send(question, channel, member);
  await until(() => said(second.reply)?.startsWith('Sure, Vikram'), 'The front desk did not answer', 30000);
  assert.equal(said(second.reply), "Sure, Vikram — I'm in the middle of Ananya's September reconciliation; I'll pull Sharma's balance right after.");
  assert.equal(app.sql(`SELECT state FROM relay_chat_jobs WHERE reply='${second.reply}'`), 'queued', 'the second request is only queued');
  assert.equal(app.sql(`SELECT decision FROM relay_front_desk WHERE reply='${second.reply}'`), 'acknowledged');
  const desk = app.provider.requests.find(r => JSON.stringify(r.messages).includes(`Front desk: ${question}`));
  const deskText = (desk?.messages ?? []).map(m => typeof m.content === 'string' ? m.content : (m.content ?? []).map(part => part.text ?? '').join('\n')).join('\n');
  assert.match(deskText, /Right now you are working on a request from Ananya Rao \(they can see that conversation\): "Reconcile September/, 'the desk knew what the agent was doing');
  assert.deepEqual((desk.tools ?? []).map(tool => tool.function?.name), [], 'the front desk has no tools');

  browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1440, height: 960 } });
  const [name, value] = member.split('=');
  await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
  const page = await context.newPage(); page.on('pageerror', error => errors.push(error.message));
  await page.goto(`${app.base}/w/${w}/?view=chat&conversation=${channel}`);
  const log = page.getByRole('log', { name: 'Messages', exact: true });
  await log.getByText("Got it. I'll match the 212 bank lines", { exact: false }).waitFor();
  await log.getByText("Sure, Vikram", { exact: false }).waitFor();
  await log.getByText('Waiting to start', { exact: true }).waitFor();
  await log.getByText('Nova is typing…', { exact: true }).waitFor();
  await page.screenshot({ path: join(app.data, 'working-voice-busy.png') });
  app.pass('The second requester got the front desk\'s answer at once, saw their own request as "Waiting to start", and typing only on the request actually running');

  app.provider.setOutputHeld(false);
  await app.done(first);
  assert.match(app.sql(`SELECT content FROM relay_chat_messages WHERE id='${first.reply}'`), /206 matched, 6 unmatched/);
  await app.done(second);
  const details = (await app.request(`${app.path}/activity/requests/${first.reply}`, app.cookie)).value;
  assert.deepEqual(details.steps.map(step => step.title), ["Said: Got it. I'll match the 212 bank lines to invoices and list anything I can't place."]);
  const deskDetails = (await app.request(`${app.path}/activity/requests/${second.reply}`, app.cookie)).value;
  assert.ok(deskDetails.steps.some(step => step.title.startsWith('Said: Sure, Vikram')), JSON.stringify(deskDetails.steps));
  assert.match(JSON.stringify(app.provider.requests.filter(r => JSON.stringify(r.messages).includes(question)).at(-1)?.messages ?? ''), /You already told the person, before starting/, 'the work lane knew not to repeat the acknowledgement');
  controllerToolsOnly(app.provider.requests.at(-1));
  // The scripted model gives every later reply the same text; the first bubble is the request's.
  await log.getByText('206 matched, 6 unmatched', { exact: false }).first().waitFor();
  assert.equal(await log.getByText('Waiting to start', { exact: true }).count(), 0);
  assert.deepEqual(errors, []);
  app.pass('The reply carried the result, the queued request then ran to completion, and Activity shows what the agent said as a step');
});
