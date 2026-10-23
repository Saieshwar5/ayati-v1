import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { appendFileSync } from 'node:fs';
import { randomUUID } from 'node:crypto';

export async function loadProfile() {
  const file = process.env.AYATI_LOAD_PROFILE_FILE;
  if (!file) return undefined;
  const profile = JSON.parse(await readFile(file, 'utf8'));
  assert.equal(profile.version, 1);
  assert.match(profile.prefix, /^ayati-load-[a-f0-9]{16}$/);
  assert.match(profile.cpu_list, /^\d+(,\d+)*$/);
  for (const role of ['driver', 'controller', 'executor', 'postgres']) {
    const limit = profile.roles[role];
    assert.ok(role === 'executor' ? [100, 200].includes(limit.cpu_percent) : limit.cpu_percent === 100);
    assert.ok(Number.isSafeInteger(limit.memory_bytes) && limit.memory_bytes >= 134217728);
    assert.equal(limit.tasks, 256);
  }
  return profile;
}

export function limitCommand(profile, role, command, args) {
  if (!profile) return { command, args };
  const limits = profile.roles[role];
  const unit = `${profile.prefix}-${role}-${randomUUID().replaceAll('-', '')}.scope`;
  appendFileSync(profile.units_file, unit + '\n', { mode: 0o600 });
  return {
    command: 'systemd-run', unit, limits,
    args: ['--user', '--scope', '--quiet', `--unit=${unit}`,
      '-p', `CPUQuota=${limits.cpu_percent}%`, '-p', `MemoryMax=${limits.memory_bytes}`,
      '-p', 'MemorySwapMax=0', '-p', `TasksMax=${limits.tasks}`,
      'env', '-u', 'DBUS_SESSION_BUS_ADDRESS', '-u', 'XDG_RUNTIME_DIR', command, ...args],
  };
}

function counters(text) {
  return Object.fromEntries(text.trim().split('\n').filter(Boolean).map(line => {
    const [key, value] = line.split(/\s+/);
    return [key, Number(value)];
  }));
}

export async function cgroupSnapshot(pid) {
  const membership = await readFile(`/proc/${pid}/cgroup`, 'utf8');
  const path = membership.match(/^0::(.+)$/m)?.[1];
  assert.ok(path?.startsWith('/') && !path.includes('..'), 'Missing cgroup v2 membership');
  const directory = '/sys/fs/cgroup' + path;
  const names = ['cpu.max', 'memory.max', 'memory.swap.max', 'pids.max',
    'cpu.stat', 'memory.current', 'memory.peak', 'memory.events'];
  const values = await Promise.all(names.map(name => readFile(`${directory}/${name}`, 'utf8')));
  const data = Object.fromEntries(names.map((name, index) => [name, values[index].trim()]));
  return { path, cpu_max: data['cpu.max'], memory_max: data['memory.max'],
    swap_max: data['memory.swap.max'], tasks_max: data['pids.max'],
    cpu: counters(data['cpu.stat']), memory_bytes: Number(data['memory.current']),
    peak_memory_bytes: Number(data['memory.peak']), memory_events: counters(data['memory.events']) };
}

export function assertLimits(snapshot, profile, role) {
  const expected = profile.roles[role];
  assert.ok(snapshot.path.split('/').at(-1).startsWith(`${profile.prefix}-${role}-`), 'Process escaped its owned resource scope');
  const [quota, period] = snapshot.cpu_max.split(' ').map(Number);
  assert.equal(quota / period, expected.cpu_percent / 100, 'CPU quota was not applied');
  assert.equal(snapshot.memory_max, String(expected.memory_bytes), 'Memory limit was not applied');
  assert.equal(snapshot.swap_max, '0');
  assert.equal(snapshot.tasks_max, String(expected.tasks));
  assert.equal(snapshot.memory_events.oom_kill, 0, 'Resource profile killed a process');
}
