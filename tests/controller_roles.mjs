import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { until } from './fixtures/polling.mjs';

// One web controller and one work controller share a database and nothing else
// (runtime/controller-roles.md). The provider is scripted; no paid model is used.
const options = { skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 300000 };
const WORK = { AYATI_CONTROLLER_ROLE: 'work' };
// A job a lost notification delayed would wait for the dispatcher's 10 second check.
const PROMPT_MS = 5000;

test('a web controller and a work controller share work through the database alone', options, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true, resumableHold: true, capacity: 4,
    workspaceLimit: 4, modelLimit: 4, leaseSeconds: 15, apiRole: 'web' });
  const { path, conversation: chat } = app;
  const jobState = job => app.sql(`SELECT state FROM relay_chat_jobs WHERE reply='${job.reply}'`);
  const ready = count => until(() => Number(app.sql("SELECT count(*) FROM relay_executor_slots WHERE state='ready'")) >= count,
    'Executors did not report ready slots', 35000);
  const completed = job => until(() => app.sql(`SELECT state FROM relay_chat_messages WHERE id='${job.reply}'`) === 'complete',
    'Reply did not complete', 45000);

  // Web alone: people sign in, read and send; agent work waits in the queue.
  await assert.rejects(fetch(app.control.url + '/v1/executors/register', { method: 'POST' }), 'Web serves no executors');
  const waiting = await app.send('Asked while no work controller runs');
  assert.ok((await app.history()).some(message => message.content === 'Asked while no work controller runs'));
  await new Promise(resolve => setTimeout(resolve, 1500));
  assert.equal(jobState(waiting), 'queued');
  app.pass('Without a work controller, chat still works and the agent request waits in the queue');

  // Work arrives with an executor and finishes what web accepted.
  let work = await app.startController(WORK);
  let executor = await app.startExecutor({ AYATI_WORKER_CONTROL_URL: work.controlUrl });
  await ready(2);
  await completed(waiting);
  assert.ok((await app.history()).some(message => message.id === waiting.reply && message.state === 'complete'));
  app.pass('A work controller that starts later runs the waiting request; web shows the reply');

  // The work controller's browser port answers health checks and nothing else.
  assert.equal((await fetch(work.base + '/health/ready')).status, 200);
  for (const page of ['/', '/index.html', '/api/identity/session', '/api/identity/simulation', '/auth/login']) {
    assert.equal((await fetch(work.base + page, { redirect: 'manual' })).status, 404, page);
  }
  app.pass('A work controller serves no pages, sign-in or browser API');

  // Requests accepted by web are picked up at once, through PostgreSQL's notification.
  const picked = [];
  for (let i = 0; i < 4; i += 1) {
    const started = Date.now();
    const job = await app.send(`Prompt pick-up ${i}`);
    await until(() => jobState(job) !== 'queued', 'Request was not picked up', 20000);
    picked.push(Date.now() - started);
    await completed(job);
  }
  assert.ok(Math.max(...picked) < PROMPT_MS, `Pick-up took ${picked} ms`);
  console.log('Pick-up after web accepted (ms):', picked.join(', '));
  app.pass('Work picks up each request web accepts without waiting for its fallback timer');

  // The live stream on web carries the reply that work saved.
  const live = await fetch(`${app.base}${path}/message-events`, { headers: { Cookie: app.cookie } });
  assert.equal(live.status, 200);
  const reader = live.body.getReader();
  const streamed = await app.send('Shown live');
  let seen = '';
  const sent = Date.now();
  while (!(seen.includes(streamed.reply) && seen.includes('"state":"complete"'))) {
    assert.ok(Date.now() - sent < 20000, 'The live stream did not show the finished reply');
    const { value, done } = await reader.read();
    assert.equal(done, false, 'The live stream closed');
    seen += Buffer.from(value).toString();
    // Keep only events that follow this request's reply.
    if (!seen.includes(streamed.reply)) seen = seen.slice(-4096);
  }
  await reader.cancel();
  app.pass('Web streams a reply live although another process produced it');

  // A schedule created through web fires on work, the only scheduler.
  const dueOnce = async (name, first_ms) => {
    const id = randomUUID();
    await app.request(`${path}/agent-profiles/${app.profile.id}/schedules`, app.cookie, { id, chat, name,
      content: 'Report once', first_ms, interval_seconds: 3600, timezone: 'UTC' });
    return id;
  };
  const occurrences = id => app.sql(`SELECT count(*) FROM relay_schedule_occurrences WHERE schedule='${id}'`);
  const schedule = await dueOnce('Split check', Date.now() + 2500);
  await until(() => occurrences(schedule) === '1', 'Schedule did not fire', 20000);
  await completed({ reply: app.sql(`SELECT reply FROM relay_schedule_occurrences WHERE schedule='${schedule}'`) });
  app.pass('A schedule saved through web fires once on the work controller');

  // Work is lost mid-reply: web keeps serving, and a replacement finishes the reply.
  app.provider.setHeld(true);
  const held = await app.send('Finish after the work controller is lost');
  await until(() => jobState(held) === 'running', 'Held reply did not start', 35000);
  await app.stop(work.record, 'SIGKILL');
  await app.stop(executor, 'SIGKILL');
  assert.equal((await app.request('/api/identity/session', app.cookie)).value.user.id.length > 0, true);
  const during = await app.send('Asked while the work controller is down');
  assert.ok((await app.history()).some(message => message.content === 'Asked while the work controller is down'));
  app.pass('Losing the work controller leaves sign-in, reading and sending working');
  work = await app.startController(WORK);
  executor = await app.startExecutor({ AYATI_WORKER_CONTROL_URL: work.controlUrl });
  app.provider.setHeld(false);
  await completed(held);
  await completed(during);
  assert.equal(app.sql(`SELECT count(*) FROM relay_worker_assignments WHERE reply='${held.reply}' AND outcome='complete'`), '1');
  app.pass('A replacement work controller finishes the interrupted reply once and runs what waited');

  // Web is lost: work keeps running schedules; web returns and shows the result.
  const unattended = await dueOnce('While web is down', Date.now() + 4000);
  await app.stopApi();
  await until(() => occurrences(unattended) === '1', 'Schedule did not fire without web', 25000);
  const ran = { reply: app.sql(`SELECT reply FROM relay_schedule_occurrences WHERE schedule='${unattended}'`) };
  await completed(ran);
  await app.startApi();
  assert.equal((await app.request('/api/identity/session', app.cookie)).value.user.id.length > 0, true);
  app.pass('Without a web controller, the work controller still runs schedules to completion');

  await app.stop(executor); await app.stop(work.record);
});
