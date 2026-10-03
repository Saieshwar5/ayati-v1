import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import net from 'node:net';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { prepareRuntimeConfig } from '../lib/runtime-config.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
const temporaryRoot = path.join(root, '.tmp');
mkdirSync(temporaryRoot, { recursive: true, mode: 0o700 });
const state = mkdtempSync(path.join(temporaryRoot, 'gateway-smoke-'));
const configPath = prepareRuntimeConfig(root, state);
const config = JSON.parse(readFileSync(configPath, 'utf8'));
const portProbe = net.createServer();
portProbe.listen(0, '127.0.0.1');
await once(portProbe, 'listening');
const port = portProbe.address().port;
await new Promise((resolve) => portProbe.close(resolve));
config.gateway.port = port;
writeFileSync(configPath, `${JSON.stringify(config, null, 2)}\n`, { mode: 0o600 });
const env = { ...process.env, AYATI_STATE_DIR: state };
let output = '';
const gateway = spawn(process.execPath, ['bin/ayati.mjs', 'start'], {
  cwd: root, env, detached: true, stdio: ['ignore', 'pipe', 'pipe'],
});
gateway.stdout.on('data', (chunk) => { output = (output + chunk).slice(-16000); });
gateway.stderr.on('data', (chunk) => { output = (output + chunk).slice(-16000); });
let startupError;
gateway.on('error', (error) => { startupError = error; });

try {
  let ready = false;
  const deadline = Date.now() + 90000;
  while (Date.now() < deadline && gateway.exitCode === null && !startupError) {
    try {
      const response = await fetch(`http://127.0.0.1:${port}/readyz`, {
        headers: { Authorization: `Bearer ${config.gateway.auth.token}` },
        signal: AbortSignal.timeout(2000),
      });
      if (response.ok) { ready = true; break; }
    } catch { /* Startup has not finished. */ }
    await delay(250);
  }
  assert.equal(ready, true, 'Gateway did not become ready.');
  const web = await fetch(`http://127.0.0.1:${port}/`, {
    signal: AbortSignal.timeout(5000),
  });
  assert.equal(web.status, 200);
  assert.match(await web.text(), /<openclaw-app|<script[^>]+src=/);
  const health = spawn(process.execPath, ['bin/ayati.mjs', 'openclaw', 'health', '--json'], {
    cwd: root, env, stdio: ['ignore', 'pipe', 'pipe'],
  });
  const timeout = setTimeout(() => health.kill('SIGTERM'), 20000);
  try {
    health.stdout.on('data', () => {});
    health.stderr.on('data', (chunk) => { output = (output + chunk).slice(-16000); });
    const [code] = await once(health, 'exit');
    assert.equal(code, 0, 'Authenticated Gateway health command failed.');
  } finally { clearTimeout(timeout); }
  const finalConfig = JSON.parse(readFileSync(configPath, 'utf8'));
  assert.equal(finalConfig.update.auto.enabled, false);
  assert.equal(finalConfig.nodeHost.autoUpdate.enabled, false);
  console.log('Gateway readiness, Control UI HTTP response, authenticated health and update policy passed.');
} catch (error) {
  console.error(output.replaceAll(config.gateway.auth.token, '[redacted]'));
  throw error;
} finally {
  if (gateway.pid && gateway.exitCode === null && gateway.signalCode === null) {
    process.kill(-gateway.pid, 'SIGTERM');
    await Promise.race([once(gateway, 'exit'), delay(5000)]);
    try { process.kill(-gateway.pid, 'SIGKILL'); } catch (error) {
      if (error.code !== 'ESRCH') throw error;
    }
  }
  rmSync(state, { recursive: true, force: true });
}
