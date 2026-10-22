import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { setTimeout as delay } from 'node:timers/promises';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { until } from './fixtures/goose_app.mjs';

test('API-only workspace and independent Goose workers retain and recover shared requests', {
  timeout: 180000, skip: !process.env.RELAY_TEST_DATABASE_URL,
}, async t => {
  const app = await workerFixture(t);
  const first = await app.send('Queued while no worker exists');
  await delay(800);
  assert.equal((await app.history()).find(m => m.id === first.reply).state, 'queued');
  assert.equal(app.provider.requests.length, 0);
  app.pass('API accepts and retains a request with no worker or model credentials');

  await app.stopApi();
  await app.startApi();
  assert.equal((await app.history()).find(m => m.id === first.reply).state, 'queued');
  app.pass('API restart preserves queued requests and authenticated conversation access');

  let worker = await app.startWorker();
  assert.match((await app.done(first)).content, /GOOSE_FIXTURE_OK/);
  assert.deepEqual(app.provider.requests.at(-1).tools ?? [], []);
  assert.equal(app.attempts(first), 1);
  assert.ok(!app.provider.requests.at(-1).messages.some(m => JSON.stringify(m).includes('WORKOS')));
  app.pass('A worker with a separate data directory and no WorkOS credentials completes the request without tools');

  const calls = app.provider.requests.length;
  await app.stopApi();
  await app.startApi();
  assert.match((await app.done(first)).content, /GOOSE_FIXTURE_OK/);
  assert.equal(app.provider.requests.length, calls);
  app.pass('Completed results survive API restart without replay');

  app.provider.setHeld(true);
  const interrupted = await app.send('Recover after graceful worker shutdown');
  await until(() => app.provider.requests.length > calls, 'Worker did not start held request');
  const gracefulChildren = await app.gooseChildren(worker);
  await app.stop(worker);
  await app.assertChildrenStopped(gracefulChildren);
  app.provider.setHeld(false);
  worker = await app.startWorker();
  assert.match((await app.done(interrupted)).content, /GOOSE_FIXTURE_OK/);
  assert.equal(app.attempts(interrupted), 2);
  app.pass('SIGTERM stops Goose and immediately releases unfinished work for a replacement worker');

  app.provider.setHeld(true);
  const beforeCrash = app.provider.requests.length;
  const crashed = await app.send('Recover after abrupt worker loss');
  await until(() => app.provider.requests.length > beforeCrash, 'Worker did not begin crash request');
  const crashedChildren = await app.gooseChildren(worker);
  await app.stop(worker, 'SIGKILL');
  await app.assertChildrenStopped(crashedChildren);
  const crashScratch = await app.scratchFiles();
  // Advance only this fixture's lease instead of waiting the production five minutes.
  app.expire(crashed);
  app.provider.setHeld(false);
  worker = await app.startWorker();
  assert.match((await app.done(crashed)).content, /GOOSE_FIXTURE_OK/);
  assert.equal(app.attempts(crashed), 2);
  app.pass('Abrupt worker loss recovers after lease expiry with one final reply');

  const competitor = await app.startWorker();
  const beforeRace = app.provider.requests.length;
  const jobs = await Promise.all(Array.from({ length: 4 }, (_, i) => app.send('Concurrent worker request ' + i)));
  await Promise.all(jobs.map(job => app.done(job)));
  assert.equal(app.provider.requests.length - beforeRace, 4);
  for (const job of jobs) assert.equal(app.attempts(job), 1);
  assert.equal((await app.history()).filter(m => m.kind === 'agent').length, 7);
  await app.stop(competitor);
  app.pass('Two worker processes claim each request once and produce no duplicate replies');

  // A second private channel's content must not enter the current conversation.
  const other = (await app.request(app.path + '/chats', app.cookie, {
    id: randomUUID(), name: 'Private thread fixture', kind: 'channel', users: [], agents: [app.profile.id],
  })).value.id;
  await app.request(`${app.path}/chats/${other}/messages`, app.cookie, { id: randomUUID(), content: 'OTHER_CHANNEL_CANARY', agent: null });
  const scoped = await app.send('Stay in this conversation');
  await app.done(scoped);
  assert.ok(!JSON.stringify(app.provider.requests.at(-1).messages).includes('OTHER_CHANNEL_CANARY'));
  app.pass('Worker context remains within its conversation');

  const page = randomUUID();
  await app.request(app.path + '/library', app.cookie, {
    id: page, request: randomUUID(), revision: 0, title: 'Worker source',
    document: { type: 'doc', content: [{ type: 'paragraph', content: [{ type: 'text', text: 'AUTHORIZED_SOURCE_CANARY' }] }] },
  });
  const item = (await app.request(app.path + '/library/' + page, app.cookie)).value;
  await app.request(`${app.path}/library/${page}/share`, app.cookie, {
    request: randomUUID(), revision: item.access_revision, visibility: 'private', people: [], agents: [app.profile.id],
  });
  const source = (await app.request(`${app.path}/library/${page}/ask`, app.cookie, {
    request: randomUUID(), revision: item.revision, agent: app.profile.id, question: 'Explain the selected source.',
  })).value;
  const sourceReply = await until(async () => (await app.history(source.chat)).find(m => m.kind === 'agent'), 'Library reply missing');
  await app.done({ chat: source.chat, reply: sourceReply.id });
  assert.match(JSON.stringify(app.provider.requests.at(-1).messages), /AUTHORIZED_SOURCE_CANARY/);
  const noSource = await app.send('Fresh context without a source');
  await app.done(noSource);
  assert.ok(!JSON.stringify(app.provider.requests.at(-1).messages).includes('AUTHORIZED_SOURCE_CANARY'));
  app.pass('Authorized Library sources work across processes and do not leak into later jobs');

  // Removing the requester's grant while generation is held must cancel publication.
  const memberCookie = await app.login(1);
  const roster = (await app.request('/api/identity/simulation')).value;
  const member = roster.employees.find(e => e.number === 1).user.id;
  await app.request(app.path + '/chat-grants', app.cookie, { user: member, agent: app.profile.id, allowed: true });
  const shared = (await app.request(app.path + '/chats', app.cookie, {
    id: randomUUID(), name: 'Revocation proof', kind: 'channel', users: [member], agents: [app.profile.id],
  })).value.id;
  app.provider.setHeld(true);
  const beforeRevoke = app.provider.requests.length;
  const revoked = await app.send('Never publish after revocation', shared, memberCookie);
  await until(() => app.provider.requests.length > beforeRevoke, 'Revocation request did not begin');
  await app.request(app.path + '/chat-grants', app.cookie, { user: member, agent: app.profile.id, allowed: false });
  await app.done(revoked, 'cancelled');
  app.provider.setHeld(false);
  assert.ok(!(await app.history(shared)).some(m => m.id === revoked.reply && m.content.includes('GOOSE_FIXTURE_OK')));
  app.pass('Revoked agent grants prevent an in-flight result from being published');

  await app.stop(worker);
  assert.deepEqual(await app.scratchFiles(), crashScratch);
  app.pass('Graceful completion removes temporary files; crash scratch stays isolated for reconciliation');
});
