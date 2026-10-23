import { readFile, readdir } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { monitorEventLoopDelay } from 'node:perf_hooks';
import os from 'node:os';
import { loadProfile, cgroupSnapshot, assertLimits } from './load_limits.mjs';

const allowed = async () => (await readFile('/proc/self/status', 'utf8')).match(/^Cpus_allowed_list:\s+(.+)$/m)[1];
function cpus(list) {
  return list.split(',').flatMap(part => {
    const [first, last = first] = part.split('-').map(Number);
    return Array.from({ length: last - first + 1 }, (_, i) => first + i);
  });
}
export async function pinLoadProcess(t, cores) {
  const original = await allowed();
  const selected = cpus(original).slice(0, cores);
  if (selected.length !== cores) throw new Error('Insufficient CPUs for the named load profile');
  const list = selected.join(',');
  execFileSync('taskset', ['--all-tasks', '--pid', '--cpu-list', list, String(process.pid)], { stdio: 'pipe' });
  t.after(() => execFileSync('taskset', ['--all-tasks', '--pid', '--cpu-list', original, String(process.pid)], { stdio: 'pipe' }));
  const profile = await loadProfile();
  const driver = profile ? await cgroupSnapshot(process.pid) : undefined;
  if (profile) {
    assertLimits(driver, profile, 'driver');
    if (list !== profile.cpu_list) throw new Error('Load affinity differs from the named resource profile');
  }
  return { cpu_list: list, requested_cores: cores, logical_cpus: os.cpus().length,
    platform: process.platform, arch: process.arch, release: os.release(), node: process.version,
    load_average: os.loadavg(), total_memory_bytes: os.totalmem(), free_memory_bytes: os.freemem(),
    resource_profile: profile, driver_cgroup: driver,
    boundary: profile ? 'Verified per-role CPU/memory cgroup ceilings and a private PostgreSQL cluster; shared host CPU/disk contention remains uncontrolled.'
      : 'Fixed CPU affinity, not a CPU/memory quota; PostgreSQL and unrelated host work remain outside this affinity.' };
}
async function proc(pid) {
  try {
    const [status, stat] = await Promise.all([readFile(`/proc/${pid}/status`, 'utf8'), readFile(`/proc/${pid}/stat`, 'utf8')]);
    const fields = stat.split(') ')[1].split(' ');
    return { pid, ticks: Number(fields[11]) + Number(fields[12]) + Number(fields[13]) + Number(fields[14]),
      rss: Number(status.match(/^VmRSS:\s+(\d+)/m)?.[1] ?? 0) * 1024 };
  } catch { return undefined; }
}
async function children(pid) {
  try {
    const threads = await readdir(`/proc/${pid}/task`);
    const lists = await Promise.all(threads.map(id => readFile(`/proc/${pid}/task/${id}/children`, 'utf8').catch(() => '')));
    return [...new Set(lists.join(' ').trim().split(/\s+/).filter(Boolean).map(Number))];
  } catch { return []; }
}
export async function sampleResources(parents) {
  const profile = await loadProfile();
  const roles = parents.map((_, i) => i === 0 ? 'driver' : i === 1 ? 'controller' : 'executor');
  if (profile) { parents = [...parents, Number(process.env.AYATI_LOAD_POSTGRES_PID)]; roles.push('postgres'); }
  const before = profile ? await Promise.all(parents.map(cgroupSnapshot)) : [];
  for (let i = 0; i < before.length; i++) assertLimits(before[i], profile, roles[i]);
  const hz = Number(execFileSync('getconf', ['CLK_TCK'], { encoding: 'utf8' }));
  const loop = monitorEventLoopDelay({ resolution: 10 }); loop.enable();
  const initial = await Promise.all(parents.map(proc));
  let stopped = false, samples = 0, peak = 0;
  const familyPeaks=parents.map(()=>0);
  const worker = (async () => {
    while (!stopped) {
      const families=(await Promise.all(parents.map(children))).map((ids,i)=>[parents[i],...ids.filter(id=>!parents.includes(id))]);
      const ids = [...new Set(families.flat())];
      const values = await Promise.all(ids.map(proc));
      for(let i=0;i<families.length;i++)familyPeaks[i]=Math.max(familyPeaks[i],values.filter(p=>p&&families[i].includes(p.pid)).reduce((sum,p)=>sum+p.rss,0));
      peak = Math.max(peak, values.reduce((sum, p) => sum + (p?.rss ?? 0), 0)); samples++;
      await new Promise(resolve => setTimeout(resolve, 200));
    }
  })();
  return async () => {
    stopped = true; await worker; loop.disable();
    const final = await Promise.all(parents.map(proc));
    const after = profile ? await Promise.all(parents.map(cgroupSnapshot)) : [];
    for (let i = 0; i < after.length; i++) assertLimits(after[i], profile, roles[i]);
    const cgroups = after.map((end, i) => ({ role: roles[i], before: before[i], after: end,
      cpu_seconds: (end.cpu.usage_usec - before[i].cpu.usage_usec) / 1e6,
      throttled_seconds: (end.cpu.throttled_usec - before[i].cpu.throttled_usec) / 1e6 }));
    return { sampled_peak_family_rss_bytes: peak, sampled_peak_by_role:familyPeaks.map((rss,i)=>({role:roles[i]==='postgres'?'postgres':i===0?'fixture_driver':i===1?'controller':`executor_${i-1}`,rss_bytes:rss})),samples, parent_cpu_seconds: final.reduce((sum, p, i) => sum + (p && initial[i] ? (p.ticks - initial[i].ticks) / hz : 0), 0),
      cgroups, event_loop_ms: { p50: loop.percentile(50) / 1e6, p95: loop.percentile(95) / 1e6, max: loop.max / 1e6 },
      scope: profile ? 'Cgroup CPU includes live descendants and PostgreSQL; per-role memory peaks are lifetime peaks. Sampled RSS may double-count shared PostgreSQL pages. Shared CPU/disk contention remains.'
        : 'Sampled live API/executor/driver families; parent CPU includes waited-child time but excludes still-live child CPU and PostgreSQL. Shared-host observations, not cloud sizing.' };
  };
}
