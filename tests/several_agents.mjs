import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { until } from './fixtures/polling.mjs';

// One message asks two agents (schema 99): real pinned Goose on pooled executors,
// scripted local inference. Each agent runs separately and answers in its own reply.
const options = { skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 150000 };

test('one message asks two agents and each answers in its own reply from its own run', options, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true });
  await app.startExecutor();
  await until(() => Number(app.sql("SELECT count(*) FROM relay_executor_slots WHERE state='ready'")) >= 2,
    'Prepared slots did not become ready', 35000);
  const nova = app.profile;
  const atlas = { ...app.profile, id: randomUUID(), name: 'Atlas', role: 'Answer stock questions.' };
  await app.request(app.path + '/agent-profiles', app.cookie, atlas);
  const channel = (await app.request(app.path + '/chats', app.cookie, {
    id: randomUUID(), name: 'Stock', kind: 'channel', users: [], agents: [nova.id, atlas.id],
  })).value.id;

  const id = randomUUID(), marker = `SEVERAL_AGENTS_${id}`;
  const body = { id, agents: [nova.id, atlas.id],
    content: `[@Nova](mention:agent:${nova.id}) [@Atlas](mention:agent:${atlas.id}) ${marker} how many boxes are left?` };
  await app.request(`${app.path}/chats/${channel}/messages`, app.cookie, body);
  await app.request(`${app.path}/chats/${channel}/messages`, app.cookie, body);
  const jobs = JSON.parse(app.sql(`SELECT json_agg(row_to_json(j) ORDER BY j.sequence) FROM (SELECT reply,payload->>'agent' AS agent,sequence FROM relay_chat_jobs WHERE request_message='${id}') j`));
  assert.deepEqual(jobs.map(job => job.agent), [nova.id, atlas.id], 'one job per agent, and the replay added none');
  for (const job of jobs) await app.done({ chat: channel, reply: job.reply });

  const history = await app.history(channel);
  const replies = history.filter(message => jobs.some(job => job.reply === message.id));
  assert.deepEqual(replies.map(message => [message.author, message.state]), [[nova.id, 'complete'], [atlas.id, 'complete']]);
  // Each agent had its own model run, told which other agent the message also asked.
  const runs = app.provider.requests.map(request => JSON.stringify(request)).filter(text => text.includes(marker));
  assert.equal(runs.length, 2, 'one provider run per asked agent');
  assert.deepEqual(runs.map(text => text.match(/the same message also asked (\w+)/)?.[1]).sort(), ['Atlas', 'Nova']);
  assert.equal(Number(app.sql(`SELECT count(*) FROM relay_agent_receipts WHERE message='${id}' AND seen_at IS NOT NULL`)), 2,
    'each agent saw the message');
  app.pass('one message, two agents, two replies');
});
