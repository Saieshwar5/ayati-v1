// Steering and repeating tasks from Chat with real Goose (people-and-agents-first,
// Phase 4; designs/agent-tasks): what the person adds to their request reaches the
// agent with its next tool result; a weekly task the person asks for starts at once,
// with no confirmation, and Cuckoo books its first run. The model is scripted;
// Goose, the controller and ayati_control are real.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { controllerToolsOnly } from './fixtures/worker_control.mjs';

test('an addition reaches the running request at its next step and a weekly task starts at once, with real Goose', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 180000,
}, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true, capacity: 1, workspaceLimit: 1, modelLimit: 1, leaseSeconds: 15 });
  const ask = 'Every Monday morning send me the sales summary';
  app.provider.setControl(ask, { steps: [
    { tool: 'open_tasks', arguments: {} },
    { tool: 'create_task', arguments: { title: 'Send the Monday sales summary', repeat: { unit: 'week', days: [1], at: '09:00' } } },
  ] });
  // The request waits for an executor; the person adds to it meanwhile.
  const job = await app.send(ask);
  await app.request(`${app.path}/activity/requests/${job.reply}/steer`, app.cookie, { text: 'Include returns as their own line' });
  const executor = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  await app.done(job);
  controllerToolsOnly(app.provider.requests.at(-1));
  const sent = JSON.stringify(app.provider.requests);
  assert.ok(sent.includes('person_added') && sent.includes('Include returns as their own line'), 'the addition reached the agent with a tool result');
  assert.ok(sent.includes('Repeating work: when the person asks for something that repeats'), 'the repeating-work rule reaches the agent');
  assert.ok(sent.includes('"timezone":"Asia/Kolkata"'), 'open_tasks tells the agent the time and time zone');
  const details = (await app.request(`${app.path}/activity/requests/${job.reply}`, app.cookie)).value;
  assert.equal(details.added[0].seen, true);

  const reply = (await app.history()).find(message => message.id === job.reply);
  assert.equal(reply.task.title, 'Send the Monday sales summary');
  assert.equal(reply.task.repeats, true);
  assert.match(reply.task.next_text, /^Mon .* 09:00$/, 'the line under the reply names the next run');
  assert.equal(app.sql(`SELECT count(*) FROM relay_task_wakeups k JOIN relay_tasks t ON t.id=k.task WHERE t.agent='${app.profile.id}' AND k.kind='repeat' AND k.state='pending'`), '1', 'Cuckoo booked the first run');
  await app.stop(executor);
  app.pass('Real Goose reads what the person added with its next tool result; the weekly task it creates starts at once with its first run booked');
});
