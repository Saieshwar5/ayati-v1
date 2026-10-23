import { test } from 'node:test';
import assert from 'node:assert/strict';
import { once } from 'node:events';
import { setTimeout as delay } from 'node:timers/promises';
import { randomUUID } from 'node:crypto';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { until } from './fixtures/goose_app.mjs';

const soakSeconds = Number(process.env.AYATI_ASSIGNMENT_SOAK_SECONDS ?? 12);
assert.ok(Number.isInteger(soakSeconds) && soakSeconds >= 12 && soakSeconds <= 240);
async function exit(record) {
  if (record.child.exitCode !== null || record.child.signalCode !== null) return record.child.exitCode;
  let timer;
  try {
    return (await Promise.race([
      once(record.child, 'exit'),
      new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('Assignment worker did not exit: ' + record.logs)), 20000); }),
    ]))[0];
  } finally { clearTimeout(timer); }
}

test('assignment launch, renewal, cancellation and crash recovery use temporary real Goose workers', {
  timeout: (soakSeconds + 180) * 1000, skip: !process.env.RELAY_TEST_DATABASE_URL,
}, async t => {
  const app = await workerFixture(t, { mode: 'assigned', runSeconds: 600, resumableHold: true });
  const active = () => Number(app.sql("SELECT count(*) FROM relay_worker_assignments WHERE state IN ('launching','running')"));
  const current = job => app.sql(`SELECT claim FROM relay_chat_jobs WHERE reply='${job.reply}'`);
  const workerPid = id => Number(app.sql(`SELECT split_part(resource_id,':',2) FROM relay_worker_assignments WHERE id='${id}'`));
  const cancel = job => app.request(`${app.path}/chats/${job.chat}/messages/${job.reply}/cancel`, app.cookie, {});

  const first = await app.send('One explicit assignment, then exit');
  assert.equal(active(), 0);
  assert.equal(app.provider.requests.length, 0);
  const poller = await app.startWorker({ AYATI_CHAT_EXECUTION: 'external' });
  await delay(1000);
  assert.equal(app.provider.requests.length, 0);
  await app.stop(poller);
  app.pass('Accepted assigned work stays durable with zero workers and is invisible to polling workers');

  let launcher = await app.startLauncher();
  await app.done(first);
  await until(() => active() === 0, 'Completed worker remained active');
  assert.equal(app.attempts(first), 1);
  const completedId = current(first);
  await app.assertChildrenStopped([workerPid(completedId)]);
  assert.deepEqual(app.provider.requests.at(-1).tools ?? [], []);
  const duplicate = await app.startAssignment(completedId);
  assert.notEqual(await exit(duplicate), 0);
  assert.equal(app.attempts(first), 1);
  app.pass('One assigned real Goose worker saves a result and exits; replaying its assignment cannot execute again');

  await app.stopApi();
  await app.startApi();
  assert.match((await app.done(first)).content, /GOOSE_FIXTURE_OK/);
  assert.equal(app.attempts(first), 1);
  app.pass('Controller/API replacement preserves completed work without replay');

  app.provider.setHeld(true);
  const beforeHeld = app.provider.requests.length;
  const held = await app.send('Keep the same owner during a long active model request');
  await until(() => app.provider.requests.length > beforeHeld, 'Assigned request did not start');
  const id = current(held);
  const initialBeat = Number(app.sql(`SELECT heartbeat_ms FROM relay_worker_assignments WHERE id='${id}'`));
  const competitor = await app.startLauncher();
  const duplicateLive = await app.startAssignment(id);
  assert.notEqual(await exit(duplicateLive), 0);
  const queued = await app.send('Do not overlap a second assignment for the same agent');
  await delay(soakSeconds * 1000);
  assert.equal(current(held), id);
  assert.equal(app.attempts(held), 1);
  assert.equal(app.attempts(queued), 0);
  assert.equal(app.provider.requests.length, beforeHeld + 1);
  assert.ok(Number(app.sql(`SELECT heartbeat_ms FROM relay_worker_assignments WHERE id='${id}'`)) > initialBeat);
  assert.equal(app.sql(`SELECT run_seconds FROM relay_chat_jobs WHERE reply='${held.reply}'`), '600');
  app.provider.setHeld(false);
  await app.done(held);
  await app.done(queued);
  await until(() => active() === 0, 'Workers did not exit after renewed request');
  await app.stop(competitor);
  app.pass(`Ownership renews through ${soakSeconds}s of real Goose work with a 6s lease; duplicate starts and overlapping agent work are blocked`);

  app.provider.setHeld(true);
  const beforeCancel = app.provider.requests.length;
  const cancelled = await app.send('Cancellation must survive restarts');
  await until(() => app.provider.requests.length > beforeCancel, 'Cancellation fixture did not start');
  const cancelId = current(cancelled);
  const pid = workerPid(cancelId);
  const forbiddenCookie = await app.login(1);
  await app.request(`${app.path}/chats/${cancelled.chat}/messages/${cancelled.reply}/cancel`, forbiddenCookie, {}, 400);
  await cancel(cancelled);
  await cancel(cancelled);
  await app.done(cancelled, 'cancelled');
  await until(() => active() === 0, 'Cancelled worker remained active');
  await app.assertChildrenStopped([pid]);
  app.provider.setHeld(false);
  await app.stopApi();
  await app.startApi();
  await delay(1000);
  assert.equal((await app.done(cancelled, 'cancelled')).content, 'Request cancelled.');
  assert.equal(app.attempts(cancelled), 1);
  app.pass('Only an authorized actor can cancel; cancelled worker stops and the request stays cancelled across restart');

  const memberCookie = await app.login(1);
  const roster = (await app.request('/api/identity/simulation')).value;
  const member = roster.employees.find(e => e.number === 1).user.id;
  await app.request(app.path + '/chat-grants', app.cookie, { user: member, agent: app.profile.id, allowed: true });
  const shared = (await app.request(app.path + '/chats', app.cookie, {
    id: randomUUID(), name: 'Assigned revocation', kind: 'channel', users: [member], agents: [app.profile.id],
  })).value.id;
  app.provider.setHeld(true);
  const beforeRevoke = app.provider.requests.length;
  const revoked = await app.send('Do not publish after revocation', shared, memberCookie);
  await until(() => app.provider.requests.length > beforeRevoke, 'Revocation request did not start');
  await app.request(app.path + '/chat-grants', app.cookie, { user: member, agent: app.profile.id, allowed: false });
  await app.done(revoked, 'cancelled');
  await until(() => active() === 0, 'Revoked worker remained active');
  app.provider.setHeld(false);
  assert.equal(app.attempts(revoked), 1);
  assert.ok(!(await app.history(shared)).some(m => m.id === revoked.reply && m.content.includes('GOOSE_FIXTURE_OK')));
  app.pass('Grant revocation cancels an assigned in-flight result without replaying the denied request');

  app.provider.setHeld(true);
  const beforeCrash = app.provider.requests.length;
  const crashed = await app.send('Recover a lost assignment worker');
  await until(() => app.provider.requests.length > beforeCrash, 'Crash request did not start');
  const crashedId = current(crashed);
  const crashedChildren = await app.gooseChildren({ child: { pid: workerPid(crashedId) } });
  assert.ok(crashedChildren.length > 0);
  process.kill(workerPid(crashedId), 'SIGKILL');
  await app.assertChildrenStopped(crashedChildren);
  await until(() => app.attempts(crashed) >= 2, 'Lost worker was not replaced');
  assert.notEqual(current(crashed), crashedId);
  app.provider.setHeld(false);
  await app.done(crashed);
  assert.equal(app.attempts(crashed), 2);
  await until(() => active() === 0, 'Replacement worker did not exit');
  app.pass('Confirmed process loss creates a new fenced attempt and exactly one final reply');

  app.provider.setHeld(true);
  const beforeTerm = app.provider.requests.length;
  const interrupted = await app.send('Drain Goose before releasing a terminated worker');
  await until(() => app.provider.requests.length > beforeTerm, 'SIGTERM fixture did not start');
  const interruptedId = current(interrupted);
  const interruptedChildren = await app.gooseChildren({ child: { pid: workerPid(interruptedId) } });
  process.kill(workerPid(interruptedId), 'SIGTERM');
  await app.assertChildrenStopped(interruptedChildren);
  await until(() => app.attempts(interrupted) >= 2, 'Graceful interruption did not recover');
  assert.equal(app.sql(`SELECT outcome FROM relay_worker_assignments WHERE id='${interruptedId}'`), 'interrupted');
  app.provider.setHeld(false);
  await app.done(interrupted);
  await until(() => active() === 0, 'Graceful replacement did not exit');
  app.pass('SIGTERM drains Goose before releasing an assignment and allows one replacement');

  // The parent-death boundary kills local child workers if the launcher itself crashes.
  app.provider.setHeld(true);
  const beforeLauncherLoss = app.provider.requests.length;
  const orphan = await app.send('Recover after local launcher loss');
  await until(() => app.provider.requests.length > beforeLauncherLoss, 'Launcher loss request did not start');
  const orphanPid = workerPid(current(orphan));
  const orphanChildren = await app.gooseChildren({ child: { pid: orphanPid } });
  await app.stop(launcher, 'SIGKILL');
  await app.assertChildrenStopped([orphanPid, ...orphanChildren]);
  // Advance only this disposable fixture lease to make recovery deterministic.
  app.expire(orphan);
  app.provider.setHeld(false);
  launcher = await app.startLauncher();
  await app.done(orphan);
  assert.equal(app.attempts(orphan), 2);
  await until(() => active() === 0, 'Recovered assignment did not exit');
  app.pass('Launcher loss leaves no worker process; a new launcher recovers durable unfinished work');

  // A queued cancellation must never allocate a worker later.
  await app.stop(launcher);
  const neverRun = await app.send('Cancel before any worker starts');
  await cancel(neverRun);
  launcher = await app.startLauncher();
  await delay(1000);
  assert.equal(app.attempts(neverRun), 0);
  assert.equal(active(), 0);
  await app.stop(launcher);
  app.pass('Queued cancellation allocates no worker, and the idle launcher retains no worker tasks');

  const timedOut = await app.send('An explicit short execution deadline fails and exits');
  app.sql(`UPDATE relay_chat_jobs SET run_seconds=1 WHERE reply='${timedOut.reply}'`);
  app.provider.setHeld(true);
  launcher = await app.startLauncher();
  await app.done(timedOut, 'failed');
  await until(() => active() === 0, 'Timed-out assignment did not exit');
  app.provider.setHeld(false);
  await app.stop(launcher);
  assert.equal(app.sql(`SELECT outcome FROM relay_worker_assignments WHERE id='${current(timedOut)}'`), 'failed');
  const pids = app.sql("SELECT split_part(resource_id,':',2) FROM relay_worker_assignments WHERE resource_id IS NOT NULL").split('\n').map(Number);
  await app.assertChildrenStopped(pids);
  app.pass('Bounded execution timeout persists a failure and releases the worker');
});
