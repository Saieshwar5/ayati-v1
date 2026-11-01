// Shared rooms with real Goose: a channel keeps one brief everyone in it reads,
// and a private question asked there is answered in the asker's direct
// conversation with the agent, never in the channel.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { until } from './fixtures/polling.mjs';

test('A channel shares one brief, and a private question is answered privately', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 180000,
}, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true });
  const agent = app.profile.id;
  const memberCookie = await app.login(1);
  const member = (await app.request('/api/identity/session', memberCookie)).value.user;
  await app.request(`${app.path}/chat-grants`, app.cookie, { user: member.id, agent, allowed: true });
  const channel = (await app.request(`${app.path}/chats`, app.cookie, { id: randomUUID(), name: 'Team', kind: 'group', users: [member.id], agents: [agent] })).value.id;
  const executor = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  const inputFor = job => {
    const message = app.sql(`SELECT request_message FROM relay_chat_jobs WHERE reply='${job.reply}'`);
    return JSON.stringify(app.provider.requests.filter(r => JSON.stringify(r).includes(message)));
  };

  // The owner writes a fact in the channel and asks the agent to remember it.
  const fact = randomUUID();
  await app.request(`${app.path}/chats/${channel}/messages`, app.cookie, { id: fact, content: 'We launch on Friday 14 November' });
  app.provider.setControl('Remember our launch date', { tool: 'save_working_context', checkpoint: 'Saved the launch date.',
    arguments: { revision: 0, objective: { text: 'Launch on Friday 14 November', sources: [fact] }, decisions: [], questions: [], corrections: [] } });
  await app.done(await app.send('Remember our launch date', channel));
  assert.equal(app.sql(`SELECT count(*)||'/'||count(*) FILTER (WHERE requester='') FROM relay_working_context WHERE chat='${channel}'`), '1/1', 'the brief belongs to the channel');

  // Another member asks in the same channel: the agent reads the same brief.
  const asked = await app.send('When do we launch?', channel, memberCookie);
  await app.done(asked);
  const input = inputFor(asked);
  assert.ok(input.includes('Shared brief for this conversation') && input.includes('Launch on Friday 14 November'));
  assert.ok(input.includes('This conversation is shared: 2 people can read your reply'));
  app.pass("Real Goose saved one brief for the channel; another member's request read it, and knew who reads replies");

  // A private question in the channel moves to the member's direct conversation.
  app.provider.setControl('What is my salary?', { tool: 'continue_privately', arguments: {}, checkpoint: "I've answered you privately." });
  app.provider.setReply('You asked in Team: What is my salary?', 'Your salary is 91,000.');
  const salary = await app.send('What is my salary?', channel, memberCookie);
  await app.done(salary);
  const [dm, reply] = (await until(() => app.sql(`SELECT chat||' '||reply FROM relay_private_continuations WHERE source='${salary.reply}'`), 'Question did not move')).split(' ');
  const answer = await until(async () => (await app.history(dm, memberCookie)).find(m => m.id === reply && m.state === 'complete'), 'Private answer missing', 35000);
  assert.equal(answer.content, 'Your salary is 91,000.');
  const room = await app.history(channel);
  assert.equal(room.find(m => m.id === salary.reply).content, "I've answered you privately.");
  assert.ok(!JSON.stringify(room).includes('91,000'), 'the private answer never reaches the channel');
  await app.request(`${app.path}/chats/${dm}/messages`, app.cookie, undefined, 400);
  const privateInput = inputFor({ reply });
  assert.ok(privateInput.includes('private_continuation') && !privateInput.includes('Shared brief for this conversation'));
  await app.stop(executor);
  app.pass('A private question asked in the channel was answered only in the member\'s direct conversation with the agent');
});
