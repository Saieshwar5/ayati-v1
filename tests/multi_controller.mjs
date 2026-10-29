import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { until } from './fixtures/polling.mjs';

const options = { skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 240000 };
const ready = (app, count = 1) => until(() => Number(app.sql("SELECT count(*) FROM relay_executor_slots WHERE state='ready'")) >= count,
  'Executors did not report ready slots', 35000);

test('two controllers on one database share work exactly once, across sessions, schedules and a controller loss', options, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true, resumableHold: true, capacity: 4,
    workspaceLimit: 4, modelLimit: 4, leaseSeconds: 15, apiEnv: { AYATI_METRICS_SECONDS: '2' } });
  const b = await app.startController();
  const path = app.path, chat = app.conversation;
  const historyAt = (controller, cookie = app.cookie) => controller.request(`${path}/chats/${chat}/messages`, cookie)
    .then(response => response.value.messages);
  const finished = (controller, job) => until(async () => (await historyAt(controller)).find(m => m.id === job.reply && m.state === 'complete'),
    'Reply did not complete', 45000);
  // Submit through a chosen controller; the reply's job is found in the shared database.
  const sendAt = async (controller, content) => {
    const id = randomUUID();
    await controller.request(`${path}/chats/${chat}/messages`, app.cookie, { id, content, agent: app.profile.id });
    return { reply: app.sql(`SELECT r.id FROM relay_chat_messages r WHERE r.chat='${chat}' AND r.kind='agent'
      AND r.sequence>(SELECT sequence FROM relay_chat_messages WHERE id='${id}') ORDER BY r.sequence LIMIT 1`) };
  };
  const executorOf = job => app.sql(`SELECT c.executor FROM relay_assignment_credentials c JOIN relay_worker_assignments a ON a.id=c.assignment WHERE a.reply='${job.reply}' AND a.outcome='complete'`);

  // Only B has an executor: work accepted by A is dispatched by B.
  const onB = await app.startExecutor({ AYATI_WORKER_CONTROL_URL: b.controlUrl }); await ready(app);
  const first = await app.send('Accepted by controller A');
  await app.done(first);
  assert.ok(executorOf(first), 'Served by the executor attached to controller B');
  // Sessions live in PostgreSQL; live state reads the same everywhere.
  const sessionA = (await app.request('/api/identity/session', app.cookie)).value.user.id;
  assert.equal((await b.request('/api/identity/session', app.cookie)).value.user.id, sessionA);
  assert.ok((await historyAt(b)).some(m => m.id === first.reply && m.state === 'complete'));
  app.pass('Work accepted by one controller is dispatched by the other; sessions and history are shared');

  // A burst with executors on both controllers: every reply completes once.
  const onA = await app.startExecutor(); await ready(app, 4);
  const burst = [];
  for (let i = 0; i < 6; i += 1) burst.push(await sendAt(i % 2 ? b : app, `Burst request ${i}`));
  for (const job of burst) await finished(b, job);
  const replies = burst.map(job => `'${job.reply}'`).join(',');
  assert.equal(app.sql(`SELECT count(*) FROM relay_worker_assignments WHERE reply IN (${replies}) AND outcome='complete'`), String(burst.length));
  assert.equal(app.sql(`SELECT count(*) FROM relay_chat_jobs WHERE reply IN (${replies}) AND state<>'finished'`), '0');
  app.pass('A burst across two dispatchers completes every reply exactly once');

  // A schedule becomes due while both controllers run their schedulers.
  const schedule = randomUUID();
  await b.request(`${path}/agent-profiles/${app.profile.id}/schedules`, app.cookie, { id: schedule, chat, name: 'Two-controller check',
    content: 'Report once', first_ms: Date.now() + 2500, interval_seconds: 3600, timezone: 'UTC' });
  await until(() => app.sql(`SELECT count(*) FROM relay_schedule_occurrences WHERE schedule='${schedule}'`) === '1', 'Schedule did not fire', 20000);
  const occurrence = { reply: app.sql(`SELECT reply FROM relay_schedule_occurrences WHERE schedule='${schedule}'`) };
  await finished(b, occurrence);
  await new Promise(resolve => setTimeout(resolve, 3000));
  assert.equal(app.sql(`SELECT count(*) FROM relay_schedule_occurrences WHERE schedule='${schedule}'`), '1');
  app.pass('A due schedule fires once although both controllers run the scheduler');

  // Controller A is lost mid-reply; the reply runs on B's executor and finishes.
  await app.stop(onA);
  app.provider.setHeld(true);
  const held = await app.send('Finish after controller A is lost');
  await until(() => app.sql(`SELECT state FROM relay_chat_jobs WHERE reply='${held.reply}'`) === 'running', 'Held reply did not start', 35000);
  await app.stop(app.apiProcess(), 'SIGKILL');
  app.provider.setHeld(false);
  await finished(b, held);
  assert.equal(app.sql(`SELECT count(*) FROM relay_worker_assignments WHERE reply='${held.reply}' AND outcome='complete'`), '1');
  app.pass('Losing a controller mid-reply loses nothing: the user reads the finished reply through the other controller');

  // Replace A while B keeps serving.
  await app.startApi();
  const replaced = await app.send('Accepted by the replacement controller A');
  await finished(b, replaced);
  app.pass('A replacement controller rejoins and its work is served while the other keeps running');

  // Measurements from the controllers' pool metric lines.
  const lines = [app.apiProcess(), b.record].flatMap(record => record.logs.split('\n').filter(line => line.includes('Pool metrics')));
  assert.ok(lines.length > 0, 'Controllers published pool metrics');
  const max = Math.max(...lines.map(line => Number(/admission_wait_max_ms: ([0-9.]+)/.exec(line)?.[1] ?? 0)));
  await writeFile(join(app.data, 'multi-controller-evidence.json'), JSON.stringify({ admission_wait_max_ms: max, metric_lines: lines.length,
    replies: burst.length + 3 }, null, 2));
  console.log('Admission wait max (ms):', max);
  await app.stop(onB); await app.stop(b.record);
});
