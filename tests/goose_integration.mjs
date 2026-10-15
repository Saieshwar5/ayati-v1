import { test } from 'node:test';
import { createRequire } from 'node:module';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { root, vmManifest, setup, until } from './fixtures/goose_app.mjs';

test('real pinned Goose streams, persists scoped identity and loads history without replay', { timeout: 60000 }, async (t) => {
  const app = await setup(t);
  const health = await app.request('/health');
  assert.equal(health.runtime, 'goose');
  const input = await app.submit('hello');
  const { task, state } = await app.done(input.id);
  assert.equal(task.status, 'completed', task.detail);
  assert.equal(state.messages.find((m) => m.role === 'assistant').content, 'GOOSE_FIXTURE_OK 🦀');
  assert.ok(app.provider.requests.length > 0);
  assert.deepEqual(app.provider.requests.at(-1).tools ?? [], []);
  const db = new DatabaseSync(join(app.data, 'ayati.sqlite'));
  const session = db.prepare('SELECT * FROM runtime_sessions').get();
  assert.ok(session.session_id);
  assert.equal(db.prepare('SELECT conversation_id FROM run_scopes WHERE task_id=?').get(input.id).conversation_id, 'local-conversation');
  db.close();
  const count = app.provider.requests.length;
  await app.stop();
  await app.start();
  const followup = await app.submit('followup');
  const result = await app.done(followup.id);
  assert.equal(result.task.status, 'completed', result.task.detail);
  assert.equal(result.state.messages.filter((m) => m.role === 'assistant').length, 2);
  assert.equal(app.provider.requests.length, count + 1);
  assert.match(JSON.stringify(app.provider.requests.at(-1).messages), /hello/);
});

test('real Goose MCP actions require an exact Allow once or Deny; decisions cannot be reused', { timeout: 60000 }, async (t) => {
  const app = await setup(t, true);
  for (const allow of [true, false]) {
    const input = await app.submit(`tool ${allow ? 'allow' : 'deny'}`);
    const permission = await until(async () => {
      const s = await app.state();
      const failed = s.tasks.find((r) => r.id === input.id && r.status === 'failed');
      if (failed) throw new Error(failed.detail);
      if (s.tasks.some((r) => r.id === input.id && r.status === 'completed')) {
        throw new Error('No permission before completion: ' + JSON.stringify({ messages: s.messages, tools: app.provider.requests.map((r) => r.tools?.map((t) => t.function.name)) }));
      }
      return s.permissions?.find((p) => p.task_id === input.id);
    }, 'No permission requested');
    const schemas = app.provider.requests.at(-1).tools.map((tool) => tool.function.name);
    assert.deepEqual(schemas, ['ayati_proof__get_demo_business']);
    await app.request(`/permissions/${permission.id}`, { allow });
    const { task, state } = await app.done(input.id);
    assert.equal(task.status, 'completed', task.detail);
    const output = state.messages.filter((m) => m.task_id === input.id && m.role === 'assistant').map((m) => m.content).join('');
    if (allow) assert.match(output, /Stock: 12/);
    else assert.doesNotMatch(output, /Stock: 12/);
    const { actions } = await app.request(`/tasks/${input.id}/actions`);
    assert.equal(actions.length, 1);
    assert.equal(actions[0].name, 'ayati_proof__get_demo_business');
    assert.equal(actions[0].evidence, 'runtime_observation');
    assert.equal(actions[0].decision, allow ? 'allowed' : 'denied');
    assert.equal(actions[0].status, allow ? 'completed' : 'denied');
    if (allow) assert.match(JSON.stringify(actions[0].result), /Stock: 12/);
    const reused = await fetch(app.base + `/permissions/${permission.id}`, {
      method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ allow: true }),
    });
    assert.equal(reused.status, 400);
  }
});

test('Stop cancels a stalled real Goose request and later input remains usable', { timeout: 60000 }, async (t) => {
  const app = await setup(t);
  const input = await app.submit('stall');
  await until(() => app.provider.requests.length > 0, 'Provider did not receive prompt');
  await app.request(`/tasks/${input.id}/stop`, {});
  assert.equal((await app.done(input.id)).task.status, 'stopped');
  const next = await app.submit('continue');
  const { task } = await app.done(next.id);
  assert.equal(task.status, 'completed', task.detail);
});

test('Stop invalidates pending owner approval and it remains invalid after restart', { timeout: 60000 }, async (t) => {
  const app = await setup(t, true);
  const input = await app.submit('tool wait');
  const permission = await until(async () => (await app.state()).permissions?.[0], 'No approval');
  await app.request(`/tasks/${input.id}/stop`, {});
  await app.done(input.id);
  await app.stop();
  await app.start();
  const response = await fetch(app.base + `/permissions/${permission.id}`, {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ allow: true }),
  });
  assert.equal(response.status, 400);
  assert.deepEqual((await app.state()).permissions, []);
});


test('real browser operates Goose chat, approval and Stop through the web app', { timeout: 60000 }, async (t) => {
  const app = await setup(t, true);
  const { chromium } = createRequire(new URL('../web/package.json', import.meta.url))('@playwright/test');
  const browser = await chromium.launch();
  t.after(() => browser.close());
  const page = await browser.newPage();
  await page.goto(app.base.replace('/api', ''));
  await page.getByRole('textbox', { name: 'Message Ayati' }).fill('tool allow');
  await page.getByRole('button', { name: 'Send message' }).click();
  await page.getByRole('button', { name: 'Allow once', exact: true }).click();
  await page.getByText(/Stock: 12/).waitFor();
  await until(async () => (await app.state()).tasks.every((r) => r.status === 'completed'), 'UI tool run did not complete');
  await page.getByRole('textbox', { name: 'Message Ayati' }).fill('stall');
  await page.getByRole('button', { name: 'Send message' }).click();
  await until(() => app.provider.requests.some((r) => JSON.stringify(r.messages).includes('stall')), 'Stalled request did not start');
  await page.getByRole('button', { name: 'Stop', exact: true }).click();
  await page.getByText('Stopped. Any files already created are still available.').waitFor();
  assert.equal((await app.state()).tasks[0].status, 'stopped');
});


test('changed tool grants cannot silently reload a saved session; startup failure closes the reply', { timeout: 60000 }, async (t) => {
  const app = await setup(t, true);
  const first = await app.submit('hello');
  assert.equal((await app.done(first.id)).task.status, 'completed');
  const calls = app.provider.requests.length;
  await app.stop();
  app.configure({ AYATI_PROOF_TOOL: '0' });
  await app.start();
  const changed = await app.submit('hello again');
  const { task, state } = await app.done(changed.id);
  assert.equal(task.status, 'failed');
  assert.match(task.detail, /settings differ/);
  assert.equal(app.provider.requests.length, calls);
  assert.equal(state.messages.find((m) => m.task_id === changed.id && m.role === 'assistant').state, 'failed');
});


if (vmManifest) {
  test('VM model gateway reports a provider failure without leaking its body and permits follow-up', { timeout: 60000 }, async (t) => {
    const app = await setup(t);
    app.provider.setUnavailable(true);
    const input = await app.submit('provider-error');
    const { task, state } = await app.done(input.id);
    assert.equal(task.status, 'failed');
    assert.match(task.detail, /Model service failed|Goose request failed/);
    assert.doesNotMatch(JSON.stringify(state), /PRIVATE_PROVIDER_FAILURE_BODY/);
    app.provider.setUnavailable(false);
    const next = await app.submit('continue');
    const recovered = await app.done(next.id);
    assert.equal(recovered.task.status, 'completed', JSON.stringify({
      detail: recovered.task.detail,
      userMessages: app.provider.requests.map((r) => r.messages.filter((m) => m.role === 'user')),
    }));
  });

  test('real guest disk persists, bridge rejects other identities, and host network stays blocked', { timeout: 60000 }, async (t) => {
    const app = await setup(t);
    await app.request('/vm/start', {});
    const manifest = JSON.parse(await readFile(vmManifest, 'utf8'));
    async function bridge(input, expectedSuccess = true, key = 'client_key') {
      const status = await app.request('/vm/status');
      const ssh = spawn('ssh', ['-T', '-F', '/dev/null', '-o', 'BatchMode=yes',
        '-o', 'StrictHostKeyChecking=yes', '-o', 'GlobalKnownHostsFile=/dev/null',
        '-o', `UserKnownHostsFile=${join(manifest.directory, 'known_hosts')}`,
        '-o', `HostKeyAlias=ayati-${manifest.agent_id}`, '-o', 'IdentitiesOnly=yes',
        '-o', 'ConnectTimeout=2', '-i', join(manifest.directory, key),
        '-p', String(status.ssh_port), 'ayati@127.0.0.1', 'echo forbidden-shell-command'],
        { stdio: ['pipe', 'pipe', 'pipe'] });
      let output = '';
      ssh.stdout.on('data', (chunk) => { output += chunk; });
      ssh.stderr.resume();
      ssh.stdin.end(JSON.stringify({ version: 1, agent_id: 'local-agent', ...input }) + '\n');
      const [code] = await once(ssh, 'close');
      if (!expectedSuccess) { assert.notEqual(code, 0); return; }
      assert.equal(code, 0);
      return JSON.parse(output);
    }
    assert.equal((await bridge({ op: 'probe' })).ready, true); // Forced command ignores the supplied shell command.
    await bridge({ op: 'probe', agent_id: 'different-agent' }, false);
    await bridge({ op: 'read_file', path: '/etc/shadow' }, false);
    await bridge({ op: 'probe' }, false, 'known_hosts'); // A public host key is not a client credential.
    const providerPort = Number(new URL(app.provider.url).port);
    const network = await bridge({ op: 'network_probe', port: providerPort });
    assert.equal(network.host_gateway_accessible, false);
    assert.equal(network.metadata_accessible, false);
    assert.equal((await bridge({ op: 'storage_probe', value: 'persistent-marker' })).value, 'persistent-marker');
    const initial = await app.request('/vm/status');
    assert.equal(initial.memory_mib, 768);
    assert.ok(initial.host_rss_kib > 0);
    const sampleStart = performance.now();
    await new Promise((resolve) => setTimeout(resolve, 1000));
    const sampled = await app.request('/vm/status');
    const idleCPU = 100 * (sampled.host_cpu_seconds - initial.host_cpu_seconds) / ((performance.now() - sampleStart) / 1000);
    console.log('VM measurements:', JSON.stringify({ ...initial, idle_cpu_percent_sample: Number(idleCPU.toFixed(2)) }));
    await app.request('/vm/stop', {});
    assert.equal((await app.request('/vm/status')).state, 'off');
    await app.request('/vm/start', {});
    assert.equal((await bridge({ op: 'storage_probe' })).value, 'persistent-marker');
    const stopped = await app.submit('stall');
    await until(() => app.provider.requests.length > 0, 'No active model request');
    console.log('VM during active model request:', JSON.stringify(await app.request('/vm/status')));
    await app.request('/vm/stop', {});
    assert.equal((await app.done(stopped.id)).task.status, 'stopped');
    assert.equal((await app.request('/vm/status')).state, 'off');
  });
}


if (vmManifest) {
  test('host crash terminates its VM and restart does not replay interrupted work', { timeout: 60000 }, async (t) => {
    const app = await setup(t);
    const input = await app.submit('stall');
    await until(() => app.provider.requests.length > 0, 'Model request did not start');
    const status = await app.request('/vm/status');
    const calls = app.provider.requests.length;
    process.kill(app.childPid(), 'SIGKILL');
    await until(async () => {
      try {
        const stat = await readFile(`/proc/${status.pid}/stat`, 'utf8');
        return stat.slice(stat.lastIndexOf(')') + 2).startsWith('Z');
      } catch (error) { if (error.code === 'ENOENT') return true; throw error; }
    }, 'VM outlived its owning host process');
    await app.start();
    assert.equal((await app.state()).tasks.find((r) => r.id === input.id).status, 'interrupted');
    assert.equal((await app.request('/vm/status')).state, 'off');
    assert.equal(app.provider.requests.length, calls);
    const next = await app.submit('continue');
    const result = await app.done(next.id);
    assert.equal(result.task.status, 'completed', result.task.detail);
  });
}
