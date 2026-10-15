import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { setup, until, vmManifest } from './fixtures/goose_app.mjs';

assert.ok(vmManifest, 'Set AYATI_VM_TEST_MANIFEST to a disposable prepared real VM; this suite never substitutes a host process.');
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const idle = { AYATI_VM_IDLE_SECONDS: '1' };

async function exited(pid) {
  try { const stat = await readFile(`/proc/${pid}/stat`, 'utf8'); return stat.slice(stat.lastIndexOf(')') + 2).startsWith('Z'); }
  catch (error) { if (error.code === 'ENOENT') return true; throw error; }
}

async function daemonResources(pid) {
  const status = await readFile(`/proc/${pid}/status`, 'utf8');
  const stat = await readFile(`/proc/${pid}/stat`, 'utf8');
  const fields = stat.slice(stat.lastIndexOf(')') + 2).split(' ');
  return { rss_kib: Number(status.match(/VmRSS:\s+(\d+)/)[1]), cpu_ticks: Number(fields[11]) + Number(fields[12]) };
}

test('real VM releases RAM when idle and saved Goose history survives event and scheduled wake', { timeout: 120000 }, async (t) => {
  const app = await setup(t, false, idle);
  const first = await app.submit('Remember this synthetic morning context');
  assert.equal((await app.done(first.id)).task.status, 'completed');
  const before = await app.request('/vm/status');
  assert.equal(before.state, 'ready');
  assert.ok(before.host_rss_kib > 0);
  const sample = performance.now();
  await until(async () => (await app.request('/vm/status')).state === 'off', 'Idle VM did not stop');
  await until(() => exited(before.pid), 'QEMU still holds resources');
  assert.equal((await app.request('/health')).status, 'ready');
  const hostIdle = await daemonResources(app.childPid());
  const idleSample = performance.now();
  await delay(1100);
  const hostAfter = await daemonResources(app.childPid());
  const ticksPerSecond = Number(execFileSync('getconf', ['CLK_TCK'], { encoding: 'utf8' }).trim());
  const hostIdleCPU = 100 * (hostAfter.cpu_ticks - hostIdle.cpu_ticks) / ticksPerSecond / ((performance.now() - idleSample) / 1000);
  assert.equal(app.provider.requests.length, 1);
  const started = performance.now();
  const event = await app.request('/intake/events', { source: 'fixture.shop', event_id: 'wake-1', content: 'Summarize a new synthetic order' });
  assert.equal((await app.done(event.id)).task.status, 'completed');
  const awake = await app.request('/vm/status');
  assert.notEqual(awake.pid, before.pid);
  assert.match(JSON.stringify(app.provider.requests.at(-1).messages), /synthetic morning context/);
  console.log('Idle/wake measurements:', JSON.stringify({ released_qemu_rss_kib: before.host_rss_kib,
    idle_to_off_ms: Math.round(idleSample - sample), event_to_completion_ms: Math.round(performance.now() - started), boot_seconds: awake.boot_seconds,
    host_idle_rss_kib: hostAfter.rss_kib, host_idle_cpu_percent_sample: Number(hostIdleCPU.toFixed(2)) }));
  await until(async () => (await app.request('/vm/status')).state === 'off', 'Second idle release did not finish');
  const schedule = { id: randomUUID(), content: 'Scheduled synthetic stock summary', run_at_ms: Date.now() + 1000 };
  await app.request('/schedules', schedule);
  const scheduled = await until(async () => (await app.request('/schedules')).schedules[0]?.last_task_id, 'Schedule did not deliver while VM was off');
  assert.equal((await app.done(scheduled)).task.status, 'completed');
  assert.equal(app.provider.requests.length, 3);
});

test('a model request and pending MCP approval each keep their real VM lease beyond the idle timeout', { timeout: 90000 }, async (t) => {
  const app = await setup(t, true, idle);
  t.after(() => {
    if (!t.passed) console.error('Lease fixture provider inputs:', JSON.stringify(app.provider.requests.map((r) => r.messages.filter((m) => m.role === 'user'))));
  });
  const model = await app.submit('stall');
  await until(() => app.provider.requests.length > 0, 'No model call');
  await delay(2200);
  const working = await app.request('/vm/status');
  assert.equal(working.state, 'ready');
  assert.ok(working.active_leases.some((lease) => lease.kind === 'run' && lease.owner === model.id));
  await app.request(`/tasks/${model.id}/stop`, {});
  const tool = await app.submit('tool wait');
  const permission = await until(async () => {
    const state = await app.state();
    const task = state.tasks.find((r) => r.id === tool.id);
    assert.ok(!['failed', 'interrupted', 'completed'].includes(task?.status), JSON.stringify(task));
    return state.permissions.find((p) => p.task_id === tool.id);
  }, 'No approval');
  await delay(2200);
  const waiting = await app.request('/vm/status');
  assert.equal(waiting.state, 'ready');
  assert.ok(waiting.active_leases.some((lease) => lease.owner === tool.id));
  await app.request(`/permissions/${permission.id}`, { allow: true });
  assert.equal((await app.done(tool.id)).task.status, 'completed');
  await until(async () => (await app.request('/vm/status')).state === 'off', 'Resolved approval did not release VM');
});

test('VM Stop while off persists its pause across a crash and parks timers/events until owner Start', { timeout: 90000 }, async (t) => {
  const app = await setup(t, false, idle);
  await app.request('/vm/stop', {});
  const event = await app.request('/intake/events', { source: 'fixture.shop', event_id: 'paused-event', content: 'Queued while paused' });
  await app.request('/schedules', { id: randomUUID(), content: 'Due while paused', run_at_ms: Date.now() - 1000 });
  await delay(1200);
  assert.equal((await app.request('/vm/status')).state, 'off');
  assert.equal((await app.request('/schedules')).schedules[0].last_task_id, null);
  await app.crash();
  await app.start();
  const paused = await app.request('/vm/status');
  assert.equal(paused.state, 'off');
  assert.equal(paused.dispatch_paused, true);
  assert.equal(app.provider.requests.length, 0);
  assert.equal((await app.state()).tasks[0].status, 'queued');
  await app.request('/vm/start', {});
  assert.equal((await app.done(event.id)).task.status, 'completed');
  const scheduled = await until(async () => (await app.request('/schedules')).schedules[0].last_task_id, 'Paused schedule did not resume');
  assert.equal((await app.done(scheduled)).task.status, 'completed');
  assert.equal(app.provider.requests.length, 2);
});

test('an event arriving during automatic guest shutdown is saved and runs after a fresh boot', { timeout: 90000 }, async (t) => {
  const app = await setup(t, false, idle);
  const first = await app.submit('Initial synthetic context');
  assert.equal((await app.done(first.id)).task.status, 'completed');
  await until(async () => (await app.request('/vm/status')).state === 'changing', 'Did not observe automatic shutdown');
  const event = { source: 'fixture.shop', event_id: 'shutdown-race', content: 'Continue after shutdown transition' };
  const accepted = await app.request('/intake/events', event);
  assert.equal((await app.done(accepted.id)).task.status, 'completed');
  assert.equal((await app.request('/intake/events', event)).id, accepted.id);
  assert.equal(app.provider.requests.length, 2);
});

test('owner Stop interrupts a real boot, cancels its queued event and a fresh chat can resume', { timeout: 90000 }, async (t) => {
  const app = await setup(t, false, idle);
  const boot = fetch(app.base + '/vm/start', { method: 'POST' });
  await until(async () => (await app.request('/vm/status')).state === 'changing', 'Did not observe guest boot');
  const event = await app.request('/intake/events', { source: 'fixture.shop', event_id: 'before-stop', content: 'Do not dispatch after Stop' });
  await app.request('/vm/stop', {});
  assert.equal((await boot).status, 400);
  assert.equal((await app.done(event.id)).task.status, 'stopped');
  assert.equal((await app.request('/vm/status')).state, 'off');
  assert.equal(app.provider.requests.length, 0);
  const fresh = await app.submit('Continue with a fresh chat');
  assert.equal((await app.done(fresh.id)).task.status, 'completed');
  assert.equal(app.provider.requests.length, 1);
});

test('a boot configuration failure is terminal for that event; repair only dispatches new work', { timeout: 90000 }, async (t) => {
  const app = await setup(t, false, idle);
  await app.stop();
  const badManifest = join(app.data, 'invalid-vm.json');
  await writeFile(badManifest, '{"version":999}', { mode: 0o600 });
  app.configure({ AYATI_VM_MANIFEST: badManifest });
  await app.start();
  const event = { source: 'fixture.shop', event_id: 'failed-boot', content: 'Synthetic event with failed boot' };
  const accepted = await app.request('/intake/events', event);
  assert.equal((await app.done(accepted.id)).task.status, 'failed');
  assert.equal(app.provider.requests.length, 0);
  await app.stop();
  app.configure({ AYATI_VM_MANIFEST: vmManifest });
  await app.start();
  assert.equal((await app.request('/intake/events', event)).status, 'failed');
  const fresh = await app.submit('Continue after repairing VM configuration');
  assert.equal((await app.done(fresh.id)).task.status, 'completed');
  assert.equal(app.provider.requests.length, 1);
});
