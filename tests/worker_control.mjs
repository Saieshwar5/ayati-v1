import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { createServer } from 'node:http';
import { once } from 'node:events';
import { readFile } from 'node:fs/promises';
import { setTimeout as delay } from 'node:timers/promises';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { until } from './fixtures/goose_app.mjs';
import { machine, executor, secret, exited } from './fixtures/worker_control.mjs';

const options = { skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 120000 };
const endpoint = (scope, operation) => `/v1/assignments/${scope.assignment}/${operation}`;

test('machine credentials bind executor/run/slot and preserve idempotent start and final receipts', options, async t => {
  const app = await workerFixture(t, { mode: 'assigned', control: true });
  const incomplete = await app.startLauncher({ AYATI_WORKER_CONTROL_URL: undefined, RELAY_DATABASE_URL: undefined });
  assert.notEqual(await exited(incomplete), 0);
  assert.match(incomplete.logs, /Local control launcher requires AYATI_WORKER_CONTROL_URL/);
  const one = await executor(app);
  const two = await executor(app);
  const job = await app.send('Prepare only this authorized context');
  const grant = await one.reserve();
  const { scope } = grant;
  const request = { scope, session: randomUUID() };
  const management = `/v1/executors/${one.registration.executor}/${one.registration.incarnation}/reserve`;
  const registration = '/v1/executors/register';
  await machine(app, registration, one.registration.token, one.registration, 403);
  await machine(app, registration, app.control.bootstrap, { ...one.registration, token: secret() }, 403);
  await machine(app, management, grant.token, { request: randomUUID(), token: secret() }, 403);
  await machine(app, management, two.registration.token, grant.reservation, 403);
  await machine(app, registration, app.control.bootstrap, one.registration, 403, { Origin: app.base });
  await machine(app, registration, app.control.bootstrap, one.registration, 403, { Cookie: app.cookie });
  await machine(app, registration, secret(), one.registration, 403);
  await machine(app, registration, '', one.registration, 401);
  const replay = await machine(app, management, one.registration.token, grant.reservation);
  assert.deepEqual(replay, scope);
  assert.equal(app.attempts(job), 1);
  for (const field of ['executor', 'incarnation', 'slot', 'assignment', 'workspace', 'agent', 'chat', 'reply', 'thread', 'generation']) {
    const forged = { ...scope, [field]: field === 'generation' ? 2 : randomUUID() };
    await machine(app, endpoint(scope, 'start'), grant.token, { ...request, scope: forged }, 403);
  }
  await machine(app, endpoint(scope, 'start'), app.control.bootstrap, request, 403);
  const started = await machine(app, endpoint(scope, 'start'), grant.token, request);
  assert.equal(started.context.agent, app.profile.id);
  assert.equal(started.context.workspace, scope.workspace);
  assert.match(started.context.request, /Prepare only this authorized context/);
  assert.match(started.context.instructions, /Nova/);
  assert.equal(started.context.version, 1);
  assert.equal(JSON.stringify(started).includes('RELAY_DATABASE_URL'), false);
  for (const credential of [grant.token, one.registration.token, app.control.bootstrap]) {
    assert.equal(JSON.stringify(started).includes(credential), false);
  }
  assert.deepEqual(await machine(app, endpoint(scope, 'start'), grant.token, request), started);
  await machine(app, endpoint(scope, 'start'), grant.token, { ...request, session: randomUUID() }, 403);
  await machine(app, endpoint(scope, 'renew'), grant.token, { ...request, session: randomUUID() }, 403);
  const completion = { assignment: request, state: 'complete', text: 'CONTROL_API_DONE' };
  await machine(app, endpoint(scope, 'complete'), grant.token, completion);
  await app.stopApi();
  await app.startApi();
  assert.equal((await machine(app, endpoint(scope, 'complete'), grant.token, completion)).state, 'complete');
  await machine(app, endpoint(scope, 'complete'), grant.token, { ...completion, text: 'CHANGED' }, 409);
  assert.equal((await app.done(job)).content, 'CONTROL_API_DONE');
  assert.equal(app.provider.requests.length, 0);
  app.pass('Independent machine authentication, every scope selector, one start session and durable completion receipt survive controller replacement');

  const expiring = await app.send('An expired capability must not start');
  const expired = await one.reserve();
  app.sql(`UPDATE relay_assignment_credentials SET expires=0 WHERE assignment='${expired.scope.assignment}'`);
  const expiredRequest = { scope: expired.scope, session: randomUUID() };
  await machine(app, endpoint(expired.scope, 'start'), expired.token, expiredRequest, 403);
  await machine(app, `/v1/executors/${two.registration.executor}/${two.registration.incarnation}/release`, two.registration.token,
    { scope: expired.scope, reason: 'lost' }, 403);
  await machine(app, `/v1/executors/${one.registration.executor}/${one.registration.incarnation}/release`, one.registration.token,
    { scope: expired.scope, reason: 'lost' });
  const replacement = await one.reserve();
  assert.notEqual(replacement.scope.assignment, expired.scope.assignment);
  assert.equal(replacement.scope.generation, 2);
  await machine(app, endpoint(expired.scope, 'complete'), expired.token, { assignment: expiredRequest, state: 'complete', text: 'STALE' }, 403);
  await app.request(`${app.path}/chats/${expiring.chat}/messages/${expiring.reply}/cancel`, app.cookie, {});
  await machine(app, endpoint(replacement.scope, 'start'), replacement.token, { scope: replacement.scope, session: randomUUID() }, 403);
  assert.equal((await app.done(expiring, 'cancelled')).content, 'Request cancelled.');
  app.pass('Expired/replaced/cancelled capabilities cannot start or publish; another executor cannot release the owner');
});

test('real Goose uses controller APIs without DB credentials through restart, cancellation and lease loss', options, async t => {
  const app = await workerFixture(t, { mode: 'assigned', control: true, resumableHold: true });
  const launcher = await app.startLauncher({ RELAY_DATABASE_URL: undefined });
  const first = await app.send('Controller APIs execute one real Goose request');
  assert.match((await app.done(first)).content, /GOOSE_FIXTURE_OK/);
  assert.equal(app.attempts(first), 1);
  assert.deepEqual(app.provider.requests.at(-1).tools ?? [], []);
  app.provider.setHeld(true);
  const count = app.provider.requests.length;
  const held = await app.send('Keep this owner through controller restart');
  await until(() => app.provider.requests.length > count, 'Controlled worker did not start');
  const workerPids = await app.gooseChildren(launcher);
  assert.equal(workerPids.length, 1);
  const workerPid = workerPids[0];
  const environment = (await readFile(`/proc/${workerPid}/environ`, 'utf8')).split('\0').filter(Boolean);
  for (const name of ['RELAY_DATABASE_URL', 'RELAY_SIMULATION_DATABASE_URL', 'WORKOS_API_KEY', 'WORK_OS_API_KEY', 'AYATI_WORKER_CONTROL_KEY_FILE']) {
    assert.equal(environment.some(pair => pair.startsWith(name + '=')), false, name);
  }
  const command = await readFile(`/proc/${workerPid}/cmdline`, 'utf8');
  assert.match(command, /--controller-assignment/);
  assert.equal(command.includes(app.control.bootstrap), false);
  const claim = app.sql(`SELECT claim FROM relay_chat_jobs WHERE reply='${held.reply}'`);
  await app.stopApi();
  await app.startApi();
  await delay(7000);
  assert.equal(app.sql(`SELECT claim FROM relay_chat_jobs WHERE reply='${held.reply}'`), claim);
  assert.equal(app.attempts(held), 1);
  assert.equal(app.provider.requests.length, count + 1);
  app.provider.setHeld(false);
  await app.done(held);
  await app.assertChildrenStopped([workerPid]);
  app.pass('Real Goose has no DB/WorkOS/bootstrap credential; the same model request and owner survive controller restart and lease renewal');

  app.provider.setHeld(true);
  const beforeCancel = app.provider.requests.length;
  const cancelled = await app.send('Cancel the controller-scoped worker');
  await until(() => app.provider.requests.length > beforeCancel, 'Cancellation worker did not start');
  const cancelledPids = await app.gooseChildren(launcher);
  const goosePids = await app.gooseChildren({ child: { pid: cancelledPids[0] } });
  await app.request(`${app.path}/chats/${cancelled.chat}/messages/${cancelled.reply}/cancel`, app.cookie, {});
  await app.done(cancelled, 'cancelled');
  await app.assertChildrenStopped([...cancelledPids, ...goosePids]);
  app.provider.setHeld(false);
  assert.equal(app.attempts(cancelled), 1);
  app.pass('Durable cancellation revokes the API capability and stops/reaps the harness without publication');

  app.provider.setHeld(true);
  const beforeLoss = app.provider.requests.length;
  const lost = await app.send('Stop inference when the controller cannot renew ownership');
  await until(() => app.provider.requests.length > beforeLoss, 'Lease loss worker did not start');
  const lostPids = await app.gooseChildren(launcher);
  const lostGoose = await app.gooseChildren({ child: { pid: lostPids[0] } });
  await app.stopApi();
  // Includes the remaining confirmed lease plus ACP cancellation/reap time.
  await app.assertChildrenStopped(lostGoose, 12000);
  assert.equal(app.provider.requests.length, beforeLoss + 1);
  assert.equal(app.attempts(lost), 1);
  await delay(1000);
  app.provider.setHeld(false);
  await app.startApi();
  await app.done(lost);
  assert.equal(app.attempts(lost), 2);
  await app.assertChildrenStopped(lostPids);
  app.pass('Controller outage beyond the safe lease stops old execution; one replacement completes after recovery');
  await app.stop(launcher);
});

test('lost start and completion responses never repeat real model inference', options, async t => {
  const app = await workerFixture(t, { mode: 'assigned', control: true });
  const registered = await executor(app);
  const job = await app.send('Exactly one inference despite lost HTTP acknowledgments');
  const grant = await registered.reserve();
  const counts = { start: 0, complete: 0 };
  const proxy = createServer(async (request, response) => {
    try {
      const chunks = [];
      for await (const chunk of request) chunks.push(chunk);
      const upstream = await fetch(app.control.url + request.url, { method: 'POST',
        headers: { Authorization: request.headers.authorization, 'Content-Type': 'application/json', Connection: 'close' }, body: Buffer.concat(chunks) });
      const bytes = Buffer.from(await upstream.arrayBuffer());
      const operation = request.url.split('/').at(-1);
      if (operation in counts) {
        counts[operation]++;
        if (counts[operation] === 1 && upstream.ok) {
          response.destroy(); // Upstream committed, but the worker receives no acknowledgment.
          return;
        }
      }
      response.writeHead(upstream.status, { 'Content-Type': 'application/json' });
      response.end(bytes);
    } catch { response.destroy(); }
  }).listen(0, '127.0.0.1');
  await once(proxy, 'listening');
  t.after(() => { proxy.closeAllConnections(); proxy.close(); });
  const worker = await app.startControlled({ url: `http://127.0.0.1:${proxy.address().port}`, scope: grant.scope, token: grant.token }, {
    AYATI_EXECUTION: 'vm', AYATI_VM_MANIFEST: '/missing/private/vm.json', AYATI_BROWSER: '1', AYATI_MEMORY: '1', AYATI_PROOF_TOOL: '1',
  });
  assert.equal(await exited(worker), 0, worker.logs);
  assert.match((await app.done(job)).content, /GOOSE_FIXTURE_OK/);
  assert.ok(counts.start >= 2);
  assert.ok(counts.complete >= 2);
  assert.equal(app.provider.requests.length, 1);
  assert.equal(app.attempts(job), 1);
  assert.deepEqual(app.provider.requests[0].tools ?? [], []);
  for (const credential of [grant.token, registered.registration.token, app.control.bootstrap]) {
    assert.equal(JSON.stringify(app.provider.requests).includes(credential), false);
  }
  assert.equal((await app.scratchFiles()).length, 0);
  app.pass('Dropped start/completion responses are retried with the same session/payload; one real inference and one final reply remain');
});
