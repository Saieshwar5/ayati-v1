import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { managedGuidanceOnly } from './fixtures/worker_control.mjs';
import { until } from './fixtures/goose_app.mjs';

test('real Goose retrieves older attributed history through a scoped controller capability', {
  timeout: 180000, skip: !process.env.RELAY_TEST_DATABASE_URL,
}, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true, leaseSeconds: 15 });
  const historyId = randomUUID();
  await app.request(`${app.path}/chats/${app.conversation}/messages`, app.cookie, {
    id: historyId, content: 'OLD_DELIVERY_EVIDENCE: Priya agreed to deliver on Friday.', agent: null,
  });
  const other = (await app.request(app.path + '/chats', app.cookie, {
    id: randomUUID(), name: 'Other private work', kind: 'channel', users: [], agents: [app.profile.id],
  })).value.id;
  const secretId = randomUUID();
  await app.request(`${app.path}/chats/${other}/messages`, app.cookie, {
    id: secretId, content: 'FOREIGN_SALARY_EVIDENCE', agent: null,
  });
  for (let index = 0; index < 70; index++) {
    await app.request(`${app.path}/chats/${app.conversation}/messages`, app.cookie, {
      id: randomUUID(), content: `Later ordinary discussion ${index}`, agent: null,
    });
  }
  const command = 'Find our older delivery agreement and verify its exact words.';
  app.provider.setControl(command, { steps: [
    { tool: 'search_conversation_history', arguments: { query: 'old_delivery_evidence' } },
    { tool: 'read_conversation_history', arguments: { message: historyId } },
    { tool: 'read_conversation_history', arguments: { message: secretId } },
    { tool: 'search_conversation_history', arguments: { query: 'FOREIGN_SALARY' } },
  ] });
  const job = await app.send(command);
  assert.ok(!job.prompt.includes('OLD_DELIVERY_EVIDENCE'));
  await app.stopApi();
  await app.startApi();
  await app.startExecutor();
  const reply = await app.done(job);
  assert.match(reply.content, /OLD_DELIVERY_EVIDENCE/);
  const requests = app.provider.requests;
  managedGuidanceOnly(app.provider.requests[0]);
  const first = JSON.stringify(requests[0].messages);
  assert.ok(!first.includes('OLD_DELIVERY_EVIDENCE'));
  const tools = requests.flatMap(request => request.messages.filter(m => m.role === 'tool').map(m => m.content));
  const toolEvidence = tools.join('\n');
  assert.match(toolEvidence, /OLD_DELIVERY_EVIDENCE/);
  assert.match(toolEvidence, /Friday/);
  assert.match(toolEvidence, /not_found/);
  assert.ok(!toolEvidence.includes('FOREIGN_SALARY_EVIDENCE'));
  const names = requests[0].tools.map(tool => tool.function.name);
  assert.ok(names.includes('ayati_control__search_conversation_history'));
  assert.ok(names.includes('ayati_control__read_conversation_history'));
  assert.equal(Number(app.sql('SELECT count(*) FROM relay_control_actions')), 0);
  assert.equal(Number(app.sql('SELECT count(*) FROM relay_chat_jobs')), 1);
  app.pass('After API restart, a real pooled Goose worker searches older evidence outside recent context and reads the exact scoped message without creating actions or new jobs');

  const roster = (await app.request('/api/identity/simulation')).value;
  const member = roster.employees.find(e => e.number === 1).user.id;
  const memberCookie = await app.login(1);
  await app.request(`${app.path}/chat-grants`, app.cookie, { user: member, agent: app.profile.id, allowed: true });
  const shared = (await app.request(app.path + '/chats', app.cookie, {
    id: randomUUID(), name: 'Revoked history access', kind: 'channel', users: [member], agents: [app.profile.id],
  })).value.id;
  app.provider.setHeld(true);
  const count = requests.length;
  const revoked = await app.send('Do not retrieve history after revocation', shared, memberCookie);
  await until(() => requests.length > count, 'Revocation request did not start');
  await app.request(`${app.path}/chat-grants`, app.cookie, { user: member, agent: app.profile.id, allowed: false });
  await app.done(revoked, 'cancelled');
  app.provider.setHeld(false);
  assert.equal(Number(app.sql('SELECT count(*) FROM relay_control_actions')), 0);
  app.pass('Current request-grant revocation cancels an active history-capable worker before it can publish a result');
  await writeFile(join(app.data, 'history-model-requests.json'), JSON.stringify(requests, null, 2), { mode: 0o600 });
});
