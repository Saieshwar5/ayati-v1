// Work an agent does at a time, with real Goose (designs/agent-tasks/one-system.md,
// decisions 1 and 4). Schedules became tasks in schema 147: a repeating task asked
// for in a channel starts at once; each run works in its own private room, its
// question and the approval of its write wait for the person who asked, the write
// happens once, and the channel gets one short result. Pausing the task stops its
// runs and keeps what was saved. The model is scripted; Goose, the controller and
// ayati_control are real.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { mkdtemp, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { remoteControlMcp } from './fixtures/remote_control_mcp.mjs';
import { until, root } from './fixtures/polling.mjs';

test('a repeating task asked for in a channel runs in its own room, asks, waits for one approval and posts one result, with real Goose', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 240000,
}, async t => {
  const remote = await remoteControlMcp(); t.after(() => remote.close());
  const directory = await mkdtemp(join(root, '.tmp/task-profile-'));
  const profiles = join(directory, 'servers.json'), bearer = join(directory, 'bearer');
  await writeFile(profiles, '[]', { mode: 0o600 }); await writeFile(bearer, remote.token, { mode: 0o600 });
  const app = await workerFixture(t, { mode: 'pooled', control: true, capacity: 1, workspaceLimit: 1, modelLimit: 1, leaseSeconds: 15, apiEnv: { AYATI_MCP_SERVERS_FILE: profiles } });
  const w = app.path.split('/').at(-1), owner = (await app.request('/api/identity/session', app.cookie)).value.user;
  const channel = (await app.request(`${app.path}/chats`, app.cookie, { id: randomUUID(), kind: 'channel', name: 'Delivery operations', users: [], agents: [app.profile.id] })).value.id;
  const state = reply => JSON.parse(app.sql(`SELECT row_to_json(j) FROM relay_chat_jobs j WHERE reply='${reply}'`));
  const channelMessages = () => Number(app.sql(`SELECT count(*) FROM relay_chat_messages WHERE chat='${channel}'`));

  const connection = randomUUID(), control = `${app.path}/agent-profiles/${app.profile.id}/control`;
  await writeFile(profiles, JSON.stringify([{ id: 'orders', workspace: w, name: 'Company orders', account: 'Test company', url: remote.url, bearer_file: bearer, operations: remote.operations }]));
  await app.request(`${app.path}/control/systems`, app.cookie, { server: 'orders', approved: true, revision: 0 });
  await app.request(`${app.path}/control/connections`, app.cookie, { id: connection, server: 'orders', enabled: true, revision: 0 });
  await app.request(`${control}/tools`, app.cookie, { connection, enabled: true });
  for (const [operation, mode] of [['order.read', 'allow'], ['order.update', 'ask']]) await app.request(`${control}/rules`, app.cookie, { connection, operation, mode, revision: 0 });

  // Asked in the channel: the task starts at once and works in a private room.
  const title = 'Hourly delivery update';
  const instructions = 'Ask for the delivery method, then prepare the separately approved order update.';
  const ask = 'Every hour, prepare the delivery update for this channel';
  app.provider.setControl(ask, { steps: [{ tool: 'create_task', arguments: { title, repeat: { unit: 'hour' }, instructions } }] });
  const specification = { version: 1, kind: 'single_choice', title: 'Delivery method', options: [{ id: 'road', label: 'Road' }, { id: 'air', label: 'Air' }] };
  const write = { tool: 'request_action', arguments: { connection, operation: 'order.update', arguments: { order: 'one', address: 'ONE_APPROVED_TASK_UPDATE' } } };
  // A run's first line names it ("Task run 1 · <title>"), which keys its script.
  app.provider.setControl(`Task run 1 · ${title}`, { steps: [{ tool: 'ask_question', arguments: { key: 'delivery', specification } }], checkpoint: 'I saved a delivery question; no order changed.',
    afterQuestion: [{ tool: 'operations', arguments: {} }, write], questionCheckpoint: 'Your answer is saved. The exact update needs separate approval.',
    after: [{ tool: 'operations', arguments: {} }, write, { tool: 'request_action', arguments: { connection, operation: 'order.read', arguments: { order: 'one' } } }] });
  const executor = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  const asked = await app.send(ask, channel);
  await app.done(asked);
  const task = JSON.parse(app.sql(`SELECT to_jsonb(t) FROM relay_tasks t WHERE t.title='${title}'`));
  assert.equal(task.chat, channel); assert.ok(task.room && task.room !== channel, 'a channel task works in its own room');
  assert.equal(task.instructions, instructions); assert.equal(task.requester, owner.id);
  assert.ok(!(await app.request(`${app.path}/chats`, app.cookie)).value.chats.some(chat => chat.id === task.room), 'the room never appears in Chat');
  app.pass('A repeating task asked for in a channel starts at once, with its own private work room');

  // Its first run is made due now; Cuckoo starts it in the room.
  app.sql(`UPDATE relay_task_wakeups SET due_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE task='${task.id}' AND state='pending' AND kind='repeat'`);
  await until(() => app.sql(`SELECT count(*) FROM relay_task_wakeups WHERE task='${task.id}' AND state='started'`) === '1', 'Cuckoo did not start the run', 20000);
  const reply = app.sql(`SELECT reply FROM relay_task_wakeups WHERE task='${task.id}' AND state='started'`);
  const job = JSON.parse(app.sql(`SELECT payload FROM relay_chat_jobs WHERE reply='${reply}'`));
  assert.equal(job.chat, task.room); assert.equal(job.requester, owner.id);
  const before = channelMessages();
  await until(() => state(reply).state === 'waiting', 'The run\'s question did not release execution', 35000);
  assert.equal(remote.calls.length, 0);
  const question = JSON.parse(app.sql(`SELECT row_to_json(q) FROM relay_task_questions q WHERE reply='${reply}'`));
  await app.request(`${app.path}/questions/${question.id}`, app.cookie, { id: randomUUID(), revision: 1, decision: 'answer', values: { choices: ['road'] } });
  await until(() => state(reply).state === 'waiting' && app.sql(`SELECT count(*) FROM relay_control_actions WHERE job='${reply}' AND state='pending'`) === '1', 'The answer did not reach a separate approval', 35000);
  assert.equal(remote.calls.length, 0);
  const action = app.sql(`SELECT id FROM relay_control_actions WHERE job='${reply}' AND state='pending'`);
  await app.request(`${control}/actions/${action}/decision`, app.cookie, { id: randomUUID(), decision: 'approve' });
  await until(() => state(reply).state === 'finished', 'The run did not finish', 35000);
  assert.equal(remote.calls.filter(call => call.name === 'update_order').length, 1); assert.equal(remote.records.get('one').address, 'ONE_APPROVED_TASK_UPDATE');
  // The channel gets exactly one short result message, linking to the run.
  await until(() => app.sql(`SELECT state FROM relay_run_posts WHERE reply='${reply}'`) === 'posted', 'The result was not posted');
  assert.equal(channelMessages(), before + 1);
  assert.match(app.sql(`SELECT content FROM relay_chat_messages WHERE chat='${channel}' ORDER BY sequence DESC LIMIT 1`), new RegExp(`View run\\]\\(run:${reply}\\)`));
  assert.equal(app.sql(`SELECT count(*) FROM relay_task_wakeups WHERE task='${task.id}' AND state='pending' AND kind='repeat'`), '1', 'the next run is booked');
  app.pass('A real Goose task run works in its room; its question and its write\'s approval wait for the person who asked; the write happens once and the channel gets one short result');

  // Pausing the task cancels its next run; a restart runs nothing again.
  const paused = (await app.request(`${app.path}/activity/tasks/${task.id}/pause`, app.cookie, { paused: true })).value;
  assert.equal(paused.paused, true);
  const calls = app.provider.requests.length;
  await app.stopApi(); await app.startApi();
  await new Promise(resolve => setTimeout(resolve, 3000));
  assert.equal(app.sql(`SELECT count(*) FROM relay_task_wakeups WHERE task='${task.id}' AND state='pending'`), '0');
  assert.equal(app.provider.requests.length, calls); assert.equal(remote.calls.filter(call => call.name === 'update_order').length, 1);
  assert.equal(app.sql(`SELECT count(*) FROM relay_agent_schedules`), '0', 'nothing is saved as a schedule');
  await app.stop(executor);
  app.pass('Pausing the task stops its runs across a restart and keeps the one confirmed write');
});
