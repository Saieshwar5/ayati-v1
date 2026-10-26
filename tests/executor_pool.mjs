import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { setTimeout as delay } from 'node:timers/promises';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { until } from './fixtures/goose_app.mjs';
import { exited, machine, secret, controllerToolsOnly } from './fixtures/worker_control.mjs';

const options = { skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 150000 };
const rows = app => JSON.parse(app.sql("SELECT COALESCE(json_agg(s ORDER BY slot),'[]') FROM relay_executor_slots s"));
const ready = (app, count = 2) => until(() => { const slots = rows(app).filter(s => s.state === 'ready'); return slots.length >= count && slots; }, 'Prepared slots did not become ready', 35000);
const claim = (app, job) => app.sql(`SELECT claim FROM relay_chat_jobs WHERE reply='${job.reply}'`);
async function secondAgent(app) {
  const profile = { ...app.profile, id: randomUUID(), name: 'Inventory', role: 'Answer only inventory questions.' };
  await app.request(app.path + '/agent-profiles', app.cookie, profile);
  const chat = (await app.request(app.path + '/chats', app.cookie, {
    id: randomUUID(), name: 'Inventory private context', kind: 'direct', users: [], agents: [profile.id],
  })).value.id;
  return { profile, chat };
}
async function twoHeld(app) {
  const other = await secondAgent(app);
  app.provider.setHeld(true);
  const before = app.provider.requests.length;
  const a = await app.send('NOVA_PRIVATE_SENTINEL: bounded sales request');
  const b = await app.send('INVENTORY_PRIVATE_SENTINEL: bounded stock request', other.chat, app.cookie, other.profile.id);
  await until(() => app.provider.requests.length >= before + 2, 'Two prepared sessions did not execute concurrently');
  return { a, b, before };
}

test('prepared slots dispatch different agents concurrently and replenish without sharing context', options, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true, resumableHold: true });
  const executor = await app.startExecutor();
  const initial = await ready(app);
  assert.equal(app.provider.requests.length, 0, 'Preparation must not run paid inference');
  assert.deepEqual((await app.gooseChildren(executor)).sort(), initial.map(s => Number(s.pid)).sort());
  const environment = (await readFile(`/proc/${executor.child.pid}/environ`, 'utf8')).split('\0');
  for (const key of ['RELAY_DATABASE_URL', 'RELAY_SIMULATION_DATABASE_URL', 'WORKOS_API_KEY']) {
    assert.equal(environment.some(pair => pair.startsWith(key + '=')), false);
  }
  const { a, b } = await twoHeld(app);
  const running = rows(app).filter(s => s.state === 'running');
  assert.equal(running.length, 2);
  const activePids = await app.gooseChildren(executor);
  assert.equal(activePids.length, 2);
  assert.ok(activePids.every(pid => !initial.some(slot => Number(slot.pid) === pid)),
    'Managed tool jobs must replace the tool-free reserve with fresh controlled sessions');
  await app.assertChildrenStopped(initial.map(slot => Number(slot.pid)));
  const queued = await app.send('Reuse a clean slot; do not expose inventory context');
  await delay(1000);
  assert.equal(app.attempts(queued), 0);
  assert.equal(app.provider.requests.length, 2);
  app.provider.setHeld(false);
  await app.done(a); await app.done(b); await app.done(queued);
  const next = await ready(app);
  assert.ok(next.every(s => s.generation > 1));
  assert.ok(next.every(s => !initial.some(old => old.pid === s.pid)));
  const last = JSON.stringify(app.provider.requests.at(-1));
  assert.equal(last.includes('INVENTORY_PRIVATE_SENTINEL'), false);
  for (const request of app.provider.requests) controllerToolsOnly(request);
  await app.assertChildrenStopped(activePids);
  assert.equal((await app.scratchFiles()).length, 2, 'Only the new ready harness scratch should remain');
  app.pass('Tool-free reserve preparation invokes no model; fresh controlled sessions serve two agents concurrently and are reaped before clean generations replace them');
  await app.stop(executor);
  await app.assertChildrenStopped(next.map(s => Number(s.pid)));
  assert.equal((await app.scratchFiles()).length, 0);
});

test('controller restart retains active pool owners and duplicate offers never replay inference', options, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true, resumableHold: true });
  const executor = await app.startExecutor(); await ready(app);
  const { a, b } = await twoHeld(app);
  const claims = [claim(app, a), claim(app, b)];
  const pids = await app.gooseChildren(executor);
  await app.stopApi(); await app.startApi();
  await delay(7000);
  assert.deepEqual([claim(app, a), claim(app, b)], claims);
  assert.equal(app.provider.requests.length, 2);
  assert.deepEqual((await app.gooseChildren(executor)).sort(), pids.sort());
  app.provider.setHeld(false); await app.done(a); await app.done(b);
  await ready(app); await app.stop(executor);
  app.pass('Executor stream reconnects after controller replacement while the original two sessions renew and finish once');
});

test('an executor crash fences and recovers every active session on another executor', options, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true, resumableHold: true });
  const persistentData = app.data + '/restarted-executor';
  const original = await app.startExecutor({ AYATI_DATA_DIR: persistentData }); await ready(app);
  const { a, b } = await twoHeld(app);
  const claims = [claim(app, a), claim(app, b)];
  const pids = await app.gooseChildren(original);
  await app.stop(original, 'SIGKILL'); await app.assertChildrenStopped(pids);
  const replacement = await app.startExecutor({ AYATI_DATA_DIR: persistentData });
  await until(() => app.attempts(a) === 2 && app.attempts(b) === 2, 'Lost executor sessions were not recovered', 25000);
  assert.notDeepEqual([claim(app, a), claim(app, b)], claims);
  assert.equal((await app.scratchFiles()).length, 2, 'Restart must reclaim the crashed incarnation scratch');
  app.provider.setHeld(false); await app.done(a); await app.done(b);
  await ready(app); await app.stop(replacement);
  assert.equal(app.attempts(a), 2); assert.equal(app.attempts(b), 2);
  app.pass('SIGKILL stops the prepared/active Goose children; a second executor independently recovers each expired generation');
});

test('draining rejects new work while active sessions finish and another executor serves the queue', options, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true, resumableHold: true });
  const executor = await app.startExecutor(); await ready(app);
  const { a, b } = await twoHeld(app);
  executor.child.kill('SIGTERM');
  await until(() => app.sql("SELECT count(*) FROM relay_executors WHERE pooled AND state='draining'") === '1', 'Executor did not enter drain');
  const queued = await app.send('Queued while the original executor drains');
  await delay(1000); assert.equal(app.attempts(queued), 0);
  app.provider.setHeld(false); await app.done(a); await app.done(b);
  assert.equal(await exited(executor), 0, executor.logs);
  assert.equal(app.attempts(queued), 0);
  const next = await app.startExecutor(); await app.done(queued); await ready(app); await app.stop(next);
  app.pass('SIGTERM preserves active work, closes ready slots and prevents new offers; another executor completes the durable queue');
});

test('failed or dead harness preparation is excluded from admission', options, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true });
  const job = await app.send('Wait for an actually prepared harness');
  const broken = await app.startExecutor({ AYATI_PREPARED_GOOSE_BIN: '/missing/goose' });
  await until(() => rows(app).length === 2, 'Preparing capacity was not reported');
  await delay(1500);
  assert.equal(rows(app).filter(s => s.state === 'ready').length, 0);
  assert.equal(app.attempts(job), 0); assert.equal(app.provider.requests.length, 0);
  await app.stop(broken);
  const healthy = await app.startExecutor(); await app.done(job);
  const initial = await ready(app); const victim = initial[0];
  process.kill(Number(victim.pid), 'SIGKILL');
  await until(() => rows(app).some(s => s.slot === victim.slot && s.generation > victim.generation && s.state === 'ready'), 'Dead prepared child was not replaced', 15000);
  await app.stop(healthy);
  app.pass('Failed preparation advertises no ready slots; an idle Goose crash withdraws and replaces that slot generation');
});

test('two executors serve separate workspaces and a recycled slot has no prior tenant context', options, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true, resumableHold: true });
  const first = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' }); await ready(app, 1);
  const second = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' }); const initial = await ready(app);
  const w = (await app.request('/api/identity/workspaces', app.cookie, { name: 'Second business', request_id: randomUUID() })).value.id;
  const path = '/api/identity/workspaces/' + w;
  const profile = { ...app.profile, id: randomUUID(), name: 'Other business', role: 'Only discuss the second business.' };
  await app.request(path + '/agent-profiles', app.cookie, profile);
  const createChat = async (kind = 'direct') => (await app.request(path + '/chats', app.cookie, {
    id: randomUUID(), name: 'Second workspace private chat', kind, users: [], agents: [profile.id],
  })).value.id;
  const sendOther = async (chat, content) => {
    await app.request(`${path}/chats/${chat}/messages`, app.cookie, { id: randomUUID(), content, agent: profile.id });
    return JSON.parse(app.sql(`SELECT payload FROM relay_chat_jobs WHERE payload->>'chat'='${chat}' ORDER BY sequence DESC LIMIT 1`));
  };
  const doneOther = async job => until(async () => (await app.request(`${path}/chats/${job.chat}/messages`, app.cookie)).value.messages.find(m => m.id === job.reply && m.state === 'complete'), 'Other workspace reply missing');
  app.provider.setHeld(true);
  const a = await app.send('TENANT_A_ONLY_SENTINEL');
  const b = await sendOther(await createChat(), 'TENANT_B_ONLY_SENTINEL');
  await until(() => app.provider.requests.length === 2, 'Independent executors did not run concurrently');
  const running = rows(app).filter(s => s.state === 'running');
  assert.equal(new Set(running.map(s => s.executor)).size, 2);
  await app.assertChildrenStopped(initial.map(s => Number(s.pid)));
  const firstPid = (await app.gooseChildren(first))[0];
  assert.ok(firstPid && !initial.some(slot => Number(slot.pid) === firstPid));
  const identity = await app.executorIdentity(first);
  const firstSlot = running.find(s => s.executor === identity.id && s.incarnation === identity.incarnation);
  assert.ok(firstSlot, 'Active slot must belong to the actual supervisor incarnation');
  const oldWorkspace = app.sql(`SELECT workspace FROM relay_worker_assignments WHERE id='${firstSlot.assignment}'`);
  app.provider.setHeld(false); await app.done(a); await doneOther(b); await ready(app);
  await app.stop(second);
  const before = app.provider.requests.length;
  if (oldWorkspace === w) {
    // Direct chats deduplicate by participants; use a group to create a distinct context.
    const freshChat = (await app.request(app.path + '/chats', app.cookie, { id: randomUUID(), name: 'Fresh A', kind: 'group', users: [], agents: [app.profile.id] })).value.id;
    assert.notEqual(freshChat, app.conversation);
    await app.done(await app.send('FRESH_OTHER_TENANT_REQUEST', freshChat));
  } else {
    const freshChat = await createChat('group');
    assert.notEqual(freshChat, b.chat);
    await doneOther(await sendOther(freshChat, 'FRESH_OTHER_TENANT_REQUEST'));
  }
  assert.equal(app.provider.requests.length, before + 1);
  const payload = JSON.stringify(app.provider.requests.at(-1));
  assert.ok(payload.includes('FRESH_OTHER_TENANT_REQUEST'));
  assert.equal(payload.includes('TENANT_A_ONLY_SENTINEL'), false);
  assert.equal(payload.includes('TENANT_B_ONLY_SENTINEL'), false);
  await app.assertChildrenStopped([firstPid]);
  await app.stop(first);
  app.pass('Two independent supervisors serve separate workspaces concurrently; sequential cross-workspace use gets a fresh session with no prior tenant transcript');
});

test('cancelling a pooled run stops its harness before the slot serves a new request', options, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true, resumableHold: true });
  const executor = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' }); const [initial] = await ready(app, 1);
  app.provider.setHeld(true);
  const cancelled = await app.send('Discard this cancelled private result');
  await until(() => app.provider.requests.length === 1, 'Held inference did not start');
  const activePids = await app.gooseChildren(executor);
  assert.equal(activePids.length, 1);
  await app.request(`${app.path}/chats/${cancelled.chat}/messages/${cancelled.reply}/cancel`, app.cookie, {});
  assert.equal((await app.done(cancelled, 'cancelled')).content, 'Request cancelled.');
  await app.assertChildrenStopped([Number(initial.pid)], 10000);
  await app.assertChildrenStopped(activePids, 10000);
  await ready(app, 1);
  assert.ok(rows(app).some(s => s.slot === initial.slot && s.generation > initial.generation));
  app.provider.setHeld(false);
  await app.done(await app.send('A new request after cancellation'));
  assert.equal((await app.done(cancelled, 'cancelled')).content, 'Request cancelled.');
  assert.equal(app.attempts(cancelled), 1);
  await app.stop(executor);
  app.pass('Cancellation fences output, reaps the used harness and prepares clean capacity without retrying the cancelled request');
});

test('a lost unacknowledged stream offer replays the exact durable assignment', options, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true });
  const e = { executor: randomUUID(), incarnation: randomUUID(), token: secret(), pooled: true };
  await machine(app, '/v1/executors/register', app.control.bootstrap, e);
  const base = `/v1/executors/${e.executor}/${e.incarnation}/pool`;
  const slot = { slot: randomUUID(), generation: 1, token: secret(), pid: 12345,
    profile: { provider: 'openai', model: 'fixture' }, state: 'ready', preparation_version: 1 };
  await machine(app, base + '/capacity', e.token, { slots: [slot] });
  const job = await app.send('Offer acknowledgment may be lost');
  async function offer() {
    const response = await fetch(app.control.url + base + '/events', {
      headers: { Authorization: `Bearer ${e.token}` }, signal: AbortSignal.timeout(5000),
    });
    assert.equal(response.status, 200);
    const reader = response.body.getReader(); let buffer = '';
    try {
      for (;;) {
        const { value, done } = await reader.read();
        assert.equal(done, false, 'Stream ended without its durable offer');
        buffer += new TextDecoder().decode(value);
        const frame = buffer.match(/data: ?([^\n]+)\n\n/);
        if (frame) return JSON.parse(frame[1]);
      }
    } finally { await reader.cancel(); }
  }
  const first = await offer(); // Drop the connection without start/ACK.
  const replay = await offer();
  assert.deepEqual(replay, first);
  assert.equal(app.attempts(job), 1);
  const request = { scope: replay, session: randomUUID() };
  const started = await machine(app, `/v1/assignments/${first.assignment}/start`, slot.token, request);
  assert.deepEqual(await machine(app, `/v1/assignments/${first.assignment}/start`, slot.token, request), started);
  await machine(app, `/v1/assignments/${first.assignment}/complete`, slot.token, { assignment: request, state: 'complete', text: 'Exactly one acknowledged output' });
  assert.equal((await app.done(job)).content, 'Exactly one acknowledged output');
  await machine(app, base + '/stop', e.token, {});
  assert.equal(app.provider.requests.length, 0, 'Protocol fixture does not fabricate model evidence');
  app.pass('Unacknowledged SSE reconnect replays one persisted offer; an old stream cannot detach its replacement and repeated start keeps one owner');
});
