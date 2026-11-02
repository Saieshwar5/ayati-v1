// Steering and routines from Chat with real Goose (people-and-agents-first, Phase 4):
// what the person adds to their request reaches the agent with its next tool result;
// a routine the agent proposes is saved only when the person confirms it. The model
// is scripted; Goose, the controller and ayati_control are real.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { controllerToolsOnly } from './fixtures/worker_control.mjs';

test('an addition reaches the running request at its next step and a proposed routine is saved by the person, with real Goose', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 180000,
}, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true, capacity: 1, workspaceLimit: 1, modelLimit: 1, leaseSeconds: 15 });
  const ask = 'Every Monday morning send me the sales summary';
  app.provider.setControl(ask, { steps: [
    { tool: 'open_tasks', arguments: {} },
    { tool: 'propose_schedule', arguments: { name: 'Monday sales summary', instructions: 'Send me last week\'s sales summary', first_in_minutes: 600, every_minutes: 10080 } },
  ] });
  // The request waits for an executor; the person adds to it meanwhile.
  const job = await app.send(ask);
  await app.request(`${app.path}/activity/requests/${job.reply}/steer`, app.cookie, { text: 'Include returns as their own line' });
  const executor = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  await app.done(job);
  controllerToolsOnly(app.provider.requests.at(-1));
  const sent = JSON.stringify(app.provider.requests);
  assert.ok(sent.includes('person_added') && sent.includes('Include returns as their own line'), 'the addition reached the agent with a tool result');
  assert.ok(sent.includes('Routines: for something that repeats'), 'the routine rule reaches the agent');
  const details = (await app.request(`${app.path}/activity/requests/${job.reply}`, app.cookie)).value;
  assert.equal(details.added[0].seen, true);

  const reply = (await app.history()).find(message => message.id === job.reply);
  assert.equal(reply.routines[0].name, 'Monday sales summary');
  assert.equal(app.sql(`SELECT count(*) FROM relay_agent_schedules WHERE agent='${app.profile.id}'`), '0', 'nothing runs until it is saved');
  const saved = (await app.request(`${app.path}/activity/routine-proposals/${reply.routines[0].id}`, app.cookie, { save: true })).value;
  assert.equal(saved.state, 'saved');
  assert.equal(app.sql(`SELECT interval_seconds FROM relay_agent_schedules WHERE id='${reply.routines[0].id}'`), '604800');
  await app.stop(executor);
  app.pass('Real Goose reads what the person added with its next tool result; its proposed weekly routine is saved only when the person confirms');
});
