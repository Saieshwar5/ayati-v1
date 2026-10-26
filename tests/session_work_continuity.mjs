import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { until } from './fixtures/goose_app.mjs';
import { ready } from './fixtures/load_workload.mjs';
import { controllerToolsOnly, managedGuidanceOnly } from './fixtures/worker_control.mjs';

test('accepted asynchronous work survives browser-session expiry while revoked authority still cancels work', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 120000,
}, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true, resumableHold: true,
    capacity: 1, workspaceLimit: 1, modelLimit: 1, leaseSeconds: 15 });
  let memberCookie = await app.login(1);
  const person = (await app.request('/api/identity/session', memberCookie)).value.user;
  await app.request(app.path + '/chat-grants', app.cookie, { user: person.id, agent: app.profile.id, allowed: true });
  const chat = (await app.request(app.path + '/chats', memberCookie, {
    id: randomUUID(), name: 'Private asynchronous work', kind: 'direct', users: [], agents: [app.profile.id],
  })).value.id;
  app.provider.setStream({ chunks: ['CONTINUED_WITHOUT_BROWSER'], holdAfter: 9, firstDelayMs: 30 });
  app.provider.setHeld(true);
  const worker = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  await ready(app, 1);
  const first = await app.send('Prepare my first private result.', chat, memberCookie);
  await until(() => app.provider.requests.length === 1, 'First accepted task did not reach inference', 35000);
  const second = await app.send('Prepare my second private result.', chat, memberCookie);
  const rows = async () => JSON.parse(await app.sqlAsync(`SELECT json_agg(row_to_json(r)) FROM (
    SELECT j.reply,j.state,j.attempts,j.claim,m.state AS message_state,m.content
    FROM relay_chat_jobs j JOIN relay_chat_messages m ON m.id=j.reply
    WHERE j.reply IN ('${first.reply}','${second.reply}') ORDER BY j.reply) r`));
  const accepted = await rows();
  assert.equal(accepted.filter(row => row.state === 'running').length, 1);
  assert.equal(accepted.filter(row => row.state === 'queued').length, 1);

  // This updates only the fixture person's sessions in its disposable schema.
  await app.sqlAsync(`UPDATE relay_sessions SET expires=0 WHERE user_id='${person.id}'`);
  await app.request(`${app.path}/chats/${chat}/messages`, memberCookie, undefined, 401);
  await app.request(`${app.path}/chats/${chat}/messages`, memberCookie,
    { id: randomUUID(), agent: app.profile.id, content: 'Expired credentials must not admit work.' }, 401);
  assert.deepEqual(await rows(), accepted, 'Expired browser reads/writes changed accepted jobs');
  await app.stopApi(); await app.startApi();
  assert.deepEqual(await rows(), accepted, 'Controller replacement changed accepted claims');
  app.provider.setHeld(false);
  const completed = await until(async () => {
    const result = await rows();
    return result.every(row => row.state === 'finished') && result;
  }, 'Accepted jobs did not complete independently of browser login', 35000);
  assert.ok(completed.every(row => row.attempts === 1 && row.message_state === 'complete'));
  assert.equal(app.provider.requests.length, 2);
  await app.request(`${app.path}/chats/${chat}/messages`, memberCookie, undefined, 401);
  await app.request(`${app.path}/chats/${chat}/messages`, app.cookie, undefined, 400);
  memberCookie = await app.login(1);
  assert.equal((await app.request('/api/identity/session', memberCookie)).value.user.id, person.id);
  const visible = await app.history(chat, memberCookie);
  for (const job of [first, second]) assert.match(visible.find(row => row.id === job.reply).content, /CONTINUED_WITHOUT_BROWSER/);
  assert.equal(app.provider.requests.length, 2, 'Signing in or reading a saved result repeated inference');
  app.pass('Running and queued work retain one attempt across browser login expiry and controller restart; expired cookies admit no new work, a different person cannot read the DM and sign-in reveals saved results without replay');

  app.provider.setHeld(true);
  const revoked = await app.send('This work must stop if my permission is removed.', chat, memberCookie);
  await until(() => app.provider.requests.length === 3, 'Revocation target did not start', 35000);
  const claim = app.sql(`SELECT claim FROM relay_chat_jobs WHERE reply='${revoked.reply}'`);
  const children = await app.assignmentProcessIds([claim]);
  await app.request(app.path + '/chat-grants', app.cookie, { user: person.id, agent: app.profile.id, allowed: false });
  await until(async () => (await app.history(chat, memberCookie)).find(row => row.id === revoked.reply && row.state === 'cancelled'),
    'Actual permission revocation did not cancel work', 35000);
  await app.assertChildrenStopped(children);
  app.provider.setHeld(false);
  const before = app.provider.requests.length;
  memberCookie = await app.login(1);
  assert.equal((await app.request('/api/identity/session', memberCookie)).value.user.id, person.id);
  await app.request(`${app.path}/chats/${chat}/messages`, memberCookie,
    { id: randomUUID(), agent: app.profile.id, content: 'A fresh login must not restore revoked permission.' }, 400);
  assert.equal(app.provider.requests.length, before);
  for (const request of app.provider.requests) { controllerToolsOnly(request); managedGuidanceOnly(request); }
  await writeFile(join(app.data, 'session-continuity.json'), JSON.stringify({
    person: person.id, chat, completed, expired_cookie_denials: 3,
    model_requests: app.provider.requests.length, revoked_reply: revoked.reply,
  }, null, 2), { mode: 0o600 });
  await ready(app, 1);
  await app.stop(worker); // Keep the restarted controller alive until the worker drains.
  app.pass('Browser session expiry and business authority are distinct: real grant revocation cancels the active worker and a new request is denied');
});
