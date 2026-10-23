import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { loadProfile, limitCommand } from './load_limits.mjs';

// Small direct probe of the pinned ACP v1 wire protocol. No host capabilities.
export async function gooseAcp(binary, data, provider) {
  const profile = await loadProfile();
  const limited = limitCommand(profile, 'executor', profile ? 'taskset' : binary,
    profile ? ['--cpu-list', profile.cpu_list, binary, 'acp'] : ['acp']);
  const env = { PATH: process.env.PATH, LANG: 'C.UTF-8',
    GOOSE_PROVIDER: 'openai', GOOSE_MODEL: 'fixture', GOOSE_PATH_ROOT: data + '/goose',
    OPENAI_API_KEY: 'fixture-not-a-secret', OPENAI_HOST: provider.url,
    GOOSE_MODE: 'approve', GOOSE_DISABLE_KEYRING: '1',
    GOOSE_DISABLE_SESSION_NAMING: 'true', DO_NOT_TRACK: '1' };
  if (profile) for (const key of ['DBUS_SESSION_BUS_ADDRESS', 'XDG_RUNTIME_DIR']) {
    if (process.env[key]) env[key] = process.env[key];
  }
  const child = spawn(limited.command, limited.args, { cwd: data, env, stdio: ['pipe', 'pipe', 'pipe'] });
  let buffer = '', logs = '', sequence = 0, unexpected = 0;
  const pending = new Map();
  const fail = error => { for (const entry of pending.values()) entry.reject(error); pending.clear(); };
  child.on('error', () => fail(new Error('ACP probe process could not start')));
  child.stdin.on('error', () => fail(new Error('ACP probe input closed')));
  child.on('exit', () => fail(new Error('ACP probe process exited')));
  child.stderr.on('data', chunk => { logs = (logs + chunk).slice(-65536); });
  child.stdout.on('data', chunk => {
    buffer += chunk;
    if (buffer.length > 1048576) { child.kill('SIGKILL'); fail(new Error('ACP probe output bound exceeded')); return; }
    for (;;) {
      const end = buffer.indexOf('\n'); if (end < 0) break;
      const raw = buffer.slice(0, end); buffer = buffer.slice(end + 1);
      if (!raw.trim()) continue;
      let message;
      try { message = JSON.parse(raw); } catch { fail(new Error('Invalid ACP probe framing')); continue; }
      if (message.method) {
        if (message.id !== undefined) {
          unexpected++;
          child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id: message.id,
            error: { code: -32601, message: 'Probe exposes no host capabilities' } }) + '\n');
        }
        continue;
      }
      const entry = pending.get(message.id);
      if (entry) {
        pending.delete(message.id);
        if (message.error) entry.reject(new Error('ACP request rejected: ' + message.error.code));
        else entry.resolve(message.result);
      }
    }
  });
  return {
    child, get logs() { return logs; }, get unexpected() { return unexpected; },
    request(method, params) {
      const id = ++sequence;
      return new Promise((resolve, reject) => {
        const timeout = setTimeout(() => { pending.delete(id); reject(new Error('ACP probe timed out')); }, 30000);
        pending.set(id, { resolve: value => { clearTimeout(timeout); resolve(value); }, reject: error => { clearTimeout(timeout); reject(error); } });
        child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id, method, params }) + '\n');
      });
    },
    async close() {
      if (child.exitCode !== null || child.signalCode !== null) return;
      const exited = once(child, 'exit'); child.kill('SIGTERM');
      const timeout = setTimeout(() => child.kill('SIGKILL'), 5000);
      try { await exited; } finally { clearTimeout(timeout); }
    },
  };
}
