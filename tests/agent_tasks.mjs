// Tasks with real Goose (people-and-agents-first, Phase 2): an agent's promise is
// saved as a task, a later request attaches to it instead of promising again, and
// the agent closes it. Chat shows the chip; Activity → Open and the task's details
// follow the records. The model is scripted; Goose, the controller and ayati_control
// are real.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { controllerToolsOnly } from './fixtures/worker_control.mjs';

test('an agent keeps a promise as a task, attaches the follow-up and closes it, with real Goose', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 180000,
}, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true, capacity: 1, workspaceLimit: 1, modelLimit: 1, leaseSeconds: 15 });
  const promise = 'Send Acme both quotes tomorrow';
  app.provider.setControl(promise, { steps: [
    { tool: 'open_tasks', arguments: {} },
    { tool: 'create_task', arguments: { title: 'Send quotes to Acme', done_when: 'Acme has both quotes' } },
  ] });
  const executor = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  const first = await app.send(promise);
  await app.done(first);
  controllerToolsOnly(app.provider.requests.at(-1));
  const sent = JSON.stringify(app.provider.requests);
  assert.ok(sent.includes('Tasks: when you commit to something that outlives this reply'), 'the promise rule reaches the agent');

  const task = app.sql("SELECT id FROM relay_tasks WHERE kind='commitment'");
  assert.ok(task, 'the promise is a task');
  assert.equal(app.sql(`SELECT relay_task_status(workspace,id) FROM relay_tasks WHERE id='${task}'`), 'open');
  const open = (await app.request(`${app.path}/activity/tasks?scope=all`, app.cookie)).value;
  assert.deepEqual(open.items.map(item => [item.title, item.status]), [['Send quotes to Acme', 'open']]);
  const reply = (await app.history()).find(message => message.id === first.reply);
  assert.equal(reply.task.id, task, 'Chat carries the task for its chip');

  const followUp = 'Did the Acme quotes go out?';
  app.provider.setControl(followUp, { steps: [
    { tool: 'open_tasks', arguments: {} },
    { tool: 'attach_to_task', arguments: { task } },
    { tool: 'close_task', arguments: { task, outcome: 'done', result: 'Both quotes sent to Acme' } },
  ] });
  const second = await app.send(followUp);
  await app.done(second);
  assert.equal(app.sql(`SELECT count(*) FROM relay_tasks WHERE kind='commitment'`), '1', 'no second promise');
  assert.equal(app.sql(`SELECT relay_task_status(workspace,id) FROM relay_tasks WHERE id='${task}'`), 'done');
  const details = (await app.request(`${app.path}/activity/tasks/${task}`, app.cookie)).value;
  assert.deepEqual(details.requests.map(step => step.id), [first.reply, second.reply]);
  assert.equal(details.result, 'Both quotes sent to Acme');
  assert.equal((await app.request(`${app.path}/activity/tasks?scope=all`, app.cookie)).value.items.length, 0, 'done tasks leave Open');
  await app.stop(executor);
  app.pass('Real Goose saves a promise as a task, attaches the follow-up instead of promising again, and closes it; Chat and Activity follow the records');
});

test('an agent books a follow-up, is woken in the same conversation for the same person, and closes the task, with real Goose', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 180000,
}, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true, capacity: 1, workspaceLimit: 1, modelLimit: 1, leaseSeconds: 15 });
  const ask = 'Call Acme back in an hour about the PO';
  app.provider.setControl(ask, { steps: [
    { tool: 'open_tasks', arguments: {} },
    { tool: 'create_task', arguments: { title: 'Call Acme back', done_when: 'Acme confirmed the PO', start_in_minutes: 60, due_in_minutes: 240 } },
  ] });
  const executor = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  const first = await app.send(ask);
  await app.done(first);
  const task = app.sql("SELECT id FROM relay_tasks WHERE kind='commitment'");
  assert.equal(app.sql(`SELECT relay_task_status(workspace,id) FROM relay_tasks WHERE id='${task}'`), 'scheduled');
  const listed = (await app.request(`${app.path}/activity/tasks?scope=all`, app.cookie)).value.items[0];
  assert.ok(listed.next_wakeup_ms > Date.now() + 50 * 60000, 'the next check is about an hour away');
  assert.ok(JSON.stringify(app.provider.requests).includes('follow_up'), 'the agent has the follow_up tool');

  // When the follow-up is due, the agent is woken and closes the task.
  app.provider.setControl('Follow-up · Call Acme back', { steps: [
    { tool: 'close_task', arguments: { task, outcome: 'done', result: 'Acme confirmed the PO on the call' } },
  ] });
  const before = app.provider.requests.length;
  app.sql(`UPDATE relay_task_wakeups SET due_ms=1 WHERE task='${task}' AND state='pending'`);
  const until = async (check, what) => { for (let i = 0; i < 300; i++) { if (await check()) return; await new Promise(r => setTimeout(r, 200)); } throw new Error(what); };
  await until(() => app.sql(`SELECT relay_task_status(workspace,id) FROM relay_tasks WHERE id='${task}'`) === 'done', 'the follow-up did not close the task');
  const woke = app.sql(`SELECT reply FROM relay_task_wakeups WHERE task='${task}' AND state='started'`);
  assert.ok(woke, 'the follow-up started a request');
  const same = field => app.sql(`SELECT (SELECT payload->>'${field}' FROM relay_chat_jobs WHERE reply='${woke}') = (SELECT payload->>'${field}' FROM relay_chat_jobs WHERE reply='${first.reply}')`);
  assert.equal(same('requester'), 't', 'for the person it promised');
  assert.equal(same('chat'), 't', 'in the same conversation');
  assert.ok(app.provider.requests.length > before);
  const history = await app.history();
  const line = history.find(m => m.follow_up);
  assert.equal(line.follow_up.id, task, 'Chat shows the follow-up as the agent following up');
  const details = (await app.request(`${app.path}/activity/tasks/${task}`, app.cookie)).value;
  assert.deepEqual(details.requests.map(step => step.id), [first.reply, woke]);
  assert.equal(details.result, 'Acme confirmed the PO on the call');
  await app.stop(executor);
  app.pass('Real Goose books a follow-up with create_task, is woken by the scheduler in the same conversation for the same person, and closes the task');
});
