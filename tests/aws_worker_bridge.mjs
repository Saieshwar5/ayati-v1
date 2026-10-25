import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { setTimeout as delay } from 'node:timers/promises';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { bridgeFixture, captureStarts, cloudEnvironment } from './fixtures/aws_bridge.mjs';
import { until } from './fixtures/goose_app.mjs';
import { machine, secret } from './fixtures/worker_control.mjs';
import { fixtureProvider } from './fixtures/goose_provider.mjs';

const options = { skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 180000 };
const slots = app => JSON.parse(app.sql("SELECT COALESCE(json_agg(s),'[]') FROM relay_executor_slots s"));
const ready = (app, count = 2) => until(() => slots(app).filter(s => s.state === 'ready').length === count,
  'Signed workers did not prepare', 35000);

test('signed registration binds the real controller and survives replacement without accepting another task', options, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true, apiEnv: cloudEnvironment });
  const bridge = await bridgeFixture(t, app);
  const registration = { executor: randomUUID(), incarnation: randomUUID(), token: secret(), pooled: true };
  const adapters = await Promise.all(bridge.workers.map(async worker => ({ control: {
    url: worker.url, bootstrap: (await readFile(worker.key_file, 'utf8')).trim(),
  } })));
  for (let replay = 0; replay < 2; replay++) {
    await machine(adapters[0], '/v1/executors/register', adapters[0].control.bootstrap, registration);
  }
  app.pass('Signed registration and its exact replay reach the real controller');
  await machine(adapters[0], '/v1/executors/register', adapters[0].control.bootstrap,
    { ...registration, task_arn: 'self-declared' }, 403);
  await machine(adapters[1], '/v1/executors/register', adapters[1].control.bootstrap, registration, 403);
  await machine(adapters[0], '/v1/executors/register', adapters[0].control.bootstrap,
    { ...registration, incarnation: randomUUID() }, 403);
  await machine(adapters[0], '/v1/executors/register', adapters[0].control.bootstrap,
    { ...registration, executor: randomUUID() }, 403);
  await machine(adapters[0], '/v1/executors/register', adapters[0].control.bootstrap,
    { ...registration, executor: randomUUID(), token: secret() }, 403);
  await machine(app, '/v1/executors/register', app.control.bootstrap, registration, 403);
  assert.equal(Number(app.sql('SELECT count(*) FROM relay_executors')), 1, 'Denied registrations must roll back');

  const path = `/v1/executors/${registration.executor}/${registration.incarnation}/pool/events`;
  async function events(url, status, extra = {}) {
    const stop = new AbortController();
    const response = await fetch(url + path, { headers: { Authorization: 'Bearer ' + registration.token, ...extra }, signal: stop.signal });
    try {
      assert.equal(response.status, status);
      if (status === 200) assert.match(new TextDecoder().decode((await response.body.getReader().read()).value), /executor/);
      else await response.arrayBuffer();
    } finally { stop.abort(); }
  }
  await events(adapters[0].control.url, 200, { 'x-ayati-verified-principal': 'forged', 'x-ayati-cloud-task': 'forged' });
  await events(adapters[1].control.url, 403);
  await events(app.control.url, 403);
  await app.stopApi();
  await app.startApi();
  await events(adapters[0].control.url, 200);
  app.pass('Actual controller registration is atomic, task-bound, replayable and durable across controller replacement');
});

test('five employee/agent identities use bounded signed workers through denial, restart and Stop', options, async t => {
  const scripted = process.env.AYATI_TEST_PREPARED_GOOSE_BIN?.endsWith('/bridge_runtime.mjs');
  console.log('Runtime under test:', scripted ? 'scripted ACP fixture; no Goose preparation acceptance' : 'pinned prepared Goose');
  const provider = await fixtureProvider({ resumableHold: true, key: secret(), heartbeat: true });
  const app = await workerFixture(t, { mode: 'pooled', control: true, resumableHold: true,
    apiEnv: cloudEnvironment, capacity: 2, provider });
  try {
    const bridge = await bridgeFixture(t, app);
    const captures = [];
    for (const worker of bridge.workers) captures.push(await captureStarts(t, worker.url));
    const capture = captures[0];
    const workers = [];
    for (const [index, worker] of bridge.workers.entries()) {
      workers.push(await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1',
        AYATI_WORKER_CONTROL_URL: captures[index].url,
        AYATI_WORKER_CONTROL_KEY_FILE: worker.key_file,
        OPENAI_HOST: worker.model_url,
        OPENAI_API_KEY: (await readFile(worker.model_key_file, 'utf8')).trim() }));
    }
    await ready(app);
    assert.equal(app.provider.requests.length, 0, 'Readiness must not call the model');
    app.pass('Two workers prepare without model requests');
    assert.equal(Number(app.sql('SELECT count(*) FROM relay_cloud_executors')), 2);
    for (const worker of workers) {
      const environment = (await readFile(`/proc/${worker.child.pid}/environ`, 'utf8')).split('\0');
      for (const name of ['RELAY_DATABASE_URL', 'RELAY_SIMULATION_DATABASE_URL', 'WORKOS_API_KEY',
        'WORK_OS_API_KEY', 'FIREWORKS_API_KEY', 'AWS_ACCESS_KEY_ID', 'AWS_SECRET_ACCESS_KEY']) {
        assert.equal(environment.some(item => item.startsWith(name + '=')), false, name);
      }
      assert.equal(environment.some(item => item.includes(provider.key)), false, 'Upstream model credential reached a worker');
    }

    const people = [];
    const roster = (await app.request('/api/identity/simulation')).value.employees;
    for (let index = 0; index < 5; index++) {
      const profile = index === 0 ? app.profile : { ...app.profile, id: randomUUID(), name: `Agent ${index}` };
      if (index) await app.request(app.path + '/agent-profiles', app.cookie, profile);
      const cookie = await app.login(index);
      if (index) {
        if (index === 1) await app.request(app.path + '/chats', cookie, {
          id: randomUUID(), name: 'Unshared agent must be denied', kind: 'direct', users: [], agents: [profile.id],
        }, 400);
        await app.request(app.path + '/chat-grants', app.cookie,
          { user: roster[index].user.id, agent: profile.id, allowed: true });
      }
      const chat = (await app.request(app.path + '/chats', cookie, {
        id: randomUUID(), name: `Employee ${index} private work`, kind: 'direct', users: [], agents: [profile.id],
      })).value.id;
      people.push({ cookie, profile, chat });
    }
    app.provider.setHeld(true);
    const jobs = await Promise.all(people.map((person, index) => app.send(
      `BRIDGE_EMPLOYEE_${index}_PRIVATE: bounded company request`, person.chat, person.cookie, person.profile.id)));
    await until(() => app.provider.requests.length === 2, 'Two signed workers did not start');
    assert.equal(slots(app).filter(s => s.state === 'running').length, 2);
    assert.equal(jobs.filter(job => app.attempts(job) === 0).length, 3);
    await until(() => capture.starts.length > 0, 'No assignment capability captured');
    const started = capture.starts[0];
    const other = { control: { url: bridge.workers[1].url } };
    await machine(other, started.path, started.token, started.body, 403);
    const owner = { control: { url: bridge.workers[0].url } };
    await machine(owner, started.path, started.token, { ...started.body,
      scope: { ...started.body.scope, workspace: randomUUID() } }, 403);
    await machine(owner, started.path, started.token, started.body);
    const direct = await fetch(app.control.url + started.path, { method: 'POST',
      headers: { Authorization: 'Bearer ' + started.token, 'Content-Type': 'application/json' },
      body: JSON.stringify(started.body) });
    assert.equal(direct.status, 403, 'A cloud capability requires its signed task binding');
    await direct.arrayBuffer();
    app.pass('Two prepared workers serve five employee/agent identities; cross-task, wrong-workspace and unsigned reuse are denied');

    const claims = jobs.map(job => app.sql(`SELECT claim FROM relay_chat_jobs WHERE reply='${job.reply}'`));
    await app.stopApi();
    await app.startApi();
    await delay(7000);
    assert.deepEqual(jobs.map(job => app.sql(`SELECT claim FROM relay_chat_jobs WHERE reply='${job.reply}'`)), claims);
    assert.equal(app.provider.requests.length, 2, 'Controller restart must retain the original model requests');
    app.provider.setHeld(false);
    for (const [index, job] of jobs.entries()) {
      const reply = await until(async () => (await app.history(job.chat, people[index].cookie))
        .find(message => message.id === job.reply && message.state === 'complete'), 'Employee reply did not finish', 35000);
      assert.match(reply.content, /GOOSE_FIXTURE_OK/);
      assert.equal(app.attempts(job), 1);
    }
    assert.equal(app.provider.requests.length, 5);
    for (const request of app.provider.requests) {
      assert.equal(new Set(JSON.stringify(request).match(/BRIDGE_EMPLOYEE_[0-4]_PRIVATE/g)).size, 1);
      assert.deepEqual(request.tools ?? [], []);
    }
    await app.request(`${app.path}/chats/${people[0].chat}/messages`, people[1].cookie, undefined, 400);
    app.pass('Controller replacement retains owners and authenticated worker bindings; each of five private requests completes once');

    await ready(app);
    app.provider.setHeld(true);
    const before = app.provider.requests.length;
    const cancelled = await app.send('Cancel through the signed worker bridge', people[0].chat);
    await until(() => app.provider.requests.length === before + 1, 'Cancellation request did not start');
    const claim = app.sql(`SELECT claim FROM relay_chat_jobs WHERE reply='${cancelled.reply}'`);
    const pids = await app.assignmentProcessIds([claim]);
    await app.request(`${app.path}/chats/${cancelled.chat}/messages/${cancelled.reply}/cancel`, app.cookie, {});
    await app.done(cancelled, 'cancelled');
    await app.assertChildrenStopped(pids);
    app.provider.setHeld(false);
    await ready(app);
    assert.equal(app.attempts(cancelled), 1);
    const completion = capture.starts.find(item => item.body.scope.assignment === claim);
    if (completion) await machine(owner, completion.path, completion.token, completion.body, 403);
    app.pass('Stop revokes the signed assignment, reaps its harness and replenishes ready capacity');

    app.provider.setHeld(true);
    const beforeCrash = app.provider.requests.length;
    const lost = await app.send('Recover one signed worker after process loss', people[0].chat);
    await until(() => app.provider.requests.length === beforeCrash + 1, 'Crash request did not start');
    const lostClaim = app.sql(`SELECT claim FROM relay_chat_jobs WHERE reply='${lost.reply}'`);
    const lostExecutor = app.sql(`SELECT executor FROM relay_assignment_credentials WHERE assignment='${lostClaim}'`);
    const identities = await Promise.all(workers.map(worker => app.executorIdentity(worker)));
    const victim = identities.findIndex(identity => identity.id === lostExecutor);
    assert.ok(victim >= 0);
    const stale = captures[victim].starts.find(item => item.body.scope.assignment === lostClaim);
    assert.ok(stale);
    const lostPids = await app.assignmentProcessIds([lostClaim]);
    await app.stop(workers[victim], 'SIGKILL');
    await app.assertChildrenStopped(lostPids);
    app.provider.setHeld(false);
    await app.done(lost);
    assert.equal(app.attempts(lost), 2);
    assert.equal(app.provider.requests.length, beforeCrash + 2);
    await machine({ control: { url: bridge.workers[victim].url } }, stale.path, stale.token, stale.body, 403);
    await ready(app, 1);
    for (const worker of workers) await app.stop(worker);
    app.pass('A dead signed worker loses ownership; its harness exits, surviving capacity recovers the request and the stale capability is denied');
  } catch (error) {
    console.error("Signed company check failed:", error.message);
    throw error;
  } finally {
    app.provider.setHeld(false);
  }
});
