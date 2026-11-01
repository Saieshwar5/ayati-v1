// The communication gateway with real Goose: a developer's transport server
// (Communication MCP) carries messages from two people to an agent; each gets
// a separate room and context, and replies go back to the right person. An
// unknown sender is held, a lost answer is settled without resending, and the
// server's token never reaches the model, logs or database.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { randomUUID } from 'node:crypto';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { transportServer } from './fixtures/transport_server.mjs';
import { until } from './fixtures/polling.mjs';
const root = resolve(import.meta.dirname, '..');

test('A developer transport carries messages to an agent and replies back, room by room', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 240000,
}, async t => {
  const platform = await transportServer(); t.after(() => platform.close());
  const directory = await mkdtemp(join(root, '.tmp/communication-gateway-'));
  const servers = join(directory, 'transport-servers.json');
  const token = join(directory, 'transport-token');
  await writeFile(token, platform.token, { mode: 0o600 });
  const app = await workerFixture(t, { mode: 'pooled', control: true, apiEnv: { AYATI_TRANSPORT_SERVERS_FILE: servers } });
  const w = app.path.split('/').at(-1);
  await writeFile(servers, JSON.stringify([{ id: 'team-chat', workspace: w, name: 'Team chat', url: platform.url, bearer_file: token }]), { mode: 0o600 });
  const path = `${app.path}/communications`;
  const owner = (await app.request('/api/identity/session', app.cookie)).value.user;
  const memberCookie = await app.login(1);
  const member = (await app.request('/api/identity/session', memberCookie)).value.user;
  await app.request(`${app.path}/chat-grants`, app.cookie, { user: member.id, agent: app.profile.id, allowed: true });

  // Admins add the agent's address; the server must serve it.
  const status = (await app.request(path, app.cookie)).value;
  assert.deepEqual(status.transports, [{ name: 'mcp', servers: [{ id: 'team-chat', name: 'Team chat' }] }]);
  const endpoint = { id: randomUUID(), agent: app.profile.id, transport: 'mcp', server: 'team-chat', address: 'nova-bot' };
  await app.request(`${path}/endpoints`, memberCookie, endpoint, 403);
  await app.request(`${path}/endpoints`, app.cookie, { ...endpoint, address: 'not-served' }, 400);
  await app.request(`${path}/endpoints`, app.cookie, endpoint);
  await app.request(`${path}/simulation/endpoints/${endpoint.id}/bindings`, app.cookie, { sender: 'U-owner', user: owner.id }, 400);
  await app.request(`${path}/endpoints/${endpoint.id}/bindings`, app.cookie, { sender: 'U-owner', user: owner.id });
  await app.request(`${path}/endpoints/${endpoint.id}/bindings`, app.cookie, { sender: 'U-member', user: member.id });
  app.pass('Admins add an address the transport server serves and say which platform users are which members');

  const inbox = id => app.sql(`SELECT status FROM relay_communication_inbox WHERE event='${id}'`);
  const reply = id => app.sql(`SELECT reply FROM relay_communication_inbox WHERE event='${id}'`);
  const state = id => app.sql(`SELECT state FROM relay_communication_outbox WHERE id='${reply(id)}'`);
  const executor = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  app.provider.setReply('PRIVATE_OWNER_SALARY_91000 How many leave days do I have?', 'You have 12 leave days.');
  app.provider.setReply('How many leave days do I have?', 'You have 8 leave days.');
  platform.message('e1', 'U-owner', 'PRIVATE_OWNER_SALARY_91000 How many leave days do I have?');
  platform.message('e2', 'U-stranger', 'Let me in');
  platform.message('e3', 'U-member', 'How many leave days do I have?', 'group');
  platform.message('e4', 'U-member', 'How many leave days do I have?');
  await until(() => platform.sent.length === 2, 'Both replies were not sent', 60000);
  const byPerson = Object.fromEntries(platform.sent.map(s => [s.to, s]));
  assert.equal(byPerson['U-owner'].text, 'You have 12 leave days.');
  assert.equal(byPerson['U-member'].text, 'You have 8 leave days.');
  assert.deepEqual([byPerson['U-owner'].conversation, byPerson['U-owner'].endpoint], ['D-U-owner', 'nova-bot']);
  assert.equal(inbox('e2'), 'held');
  assert.equal(inbox('e3'), '', 'a group message is never admitted');
  assert.equal(app.sql(`SELECT count(DISTINCT chat) FROM relay_communication_inbox WHERE transport='mcp' AND chat IS NOT NULL`), '2');
  const memberMessage = app.sql(`SELECT message FROM relay_communication_inbox WHERE event='e4'`);
  const memberInput = app.provider.requests.filter(r => JSON.stringify(r).includes(memberMessage));
  assert.ok(memberInput.length > 0);
  assert.ok(!JSON.stringify(memberInput).includes('PRIVATE_OWNER_SALARY_91000'), "the member's request never sees the owner's room");
  assert.ok(!JSON.stringify(app.provider.requests).includes('fetch_inbound'), 'the model never sees the transport server');
  await until(() => state('e1') === 'accepted' && state('e4') === 'accepted', 'Sends were not recorded');
  app.pass('Real Goose answered two people in separate rooms; replies went back to each; strangers and groups got nothing');

  // The answer to a send is lost: unclear, then settled by asking. Never resent.
  platform.setMode('lose');
  app.provider.setReply('One more question', 'One more answer.');
  platform.message('e5', 'U-owner', 'One more question');
  await until(() => state('e5') === 'uncertain', 'Send did not become unclear', 60000);
  platform.setMode('accept');
  await app.stopApi(); await app.startApi();
  app.sql(`UPDATE relay_communication_outbox SET lease_until=0 WHERE id='${reply('e5')}'`);
  await until(() => state('e5') === 'delivered', 'Unclear send was not settled', 30000);
  assert.equal(platform.sent.filter(s => s.text === 'One more answer.').length, 1);
  assert.equal(app.sql(`SELECT count(*) FROM relay_communication_inbox WHERE transport='mcp'`), '4', 'nothing was read twice after the restart');
  app.pass('A lost answer was settled by asking the platform after a restart, without sending twice');

  // Switching the endpoint off stops reading and sending.
  await app.request(`${path}/endpoints/${endpoint.id}/state`, app.cookie, { revision: 1, enabled: false });
  platform.message('e6', 'U-owner', 'After switching off');
  await new Promise(r => setTimeout(r, 7000));
  assert.equal(inbox('e6'), '');
  await app.stop(executor);
  for (const table of ['relay_communication_endpoints', 'relay_communication_inbox', 'relay_communication_outbox', 'relay_communication_cursors']) {
    assert.equal(app.sql(`SELECT count(*) FROM ${table} r WHERE to_jsonb(r)::text LIKE '%${platform.token}%'`), '0', table);
  }
  assert.ok(!app.processes().some(p => p.logs.includes(platform.token)), 'the token never reaches logs');
  assert.ok(!JSON.stringify(app.provider.requests).includes(platform.token));
  app.pass('Switching the endpoint off stopped reading; the transport token never reached the model, logs or database');
});
