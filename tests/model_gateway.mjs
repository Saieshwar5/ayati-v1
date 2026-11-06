// The model gateway (core-agent CA3) with real Goose: executors hold no model key; the
// controller picks the assignment's model, adds the provider key from its own file,
// streams the answer back and records token use. The model is scripted; Goose, the
// controller, the executor's loopback proxy and worker control are real.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { fixtureProvider } from './fixtures/goose_provider.mjs';

const root = resolve(import.meta.dirname, '..');

test('agents reach the model only through the controller, which holds the key and counts tokens', { skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 180000 }, async t => {
  const secret = 'provider-key-only-the-controller-reads';
  const provider = await fixtureProvider({ key: secret });
  const directory = await mkdtemp(join(root, '.tmp/model-gateway-'));
  const keyFile = join(directory, 'openai.key'), providers = join(directory, 'providers.json');
  await writeFile(keyFile, secret + '\n', { mode: 0o600 });
  await writeFile(providers, JSON.stringify([{ provider: 'openai', base_url: `${provider.url}/v1`, key_file: keyFile }]), { mode: 0o600 });
  const app = await workerFixture(t, { mode: 'pooled', control: true, capacity: 1, workspaceLimit: 1, modelLimit: 1, leaseSeconds: 15, provider, apiEnv: { AYATI_MODEL_PROVIDERS_FILE: providers } });
  // No key and no provider address on the executor.
  const executor = await app.startExecutor({ AYATI_MODEL_GATEWAY: '1', OPENAI_API_KEY: undefined, OPENAI_HOST: undefined, AYATI_EXECUTOR_SLOTS: '1' });
  const plain = await app.send('Say hello through the gateway');
  await app.done(plain);
  app.provider.setControl('Check my open tasks', { steps: [{ tool: 'open_tasks', arguments: {} }] });
  const tools = await app.send('Check my open tasks');
  await app.done(tools);
  assert.ok(app.provider.requests.length >= 2, 'the provider answered both runs, so the key was right');
  assert.ok(app.provider.requests.every(r => r.model === 'fixture'), "the assignment's model is used");
  const usage = app.sql("SELECT count(*) || ':' || COALESCE(sum(input_tokens),0) FROM relay_model_usage WHERE agent IS NOT NULL").split(':').map(Number);
  assert.ok(usage[0] >= 2, `usage rows ${usage[0]}`);
  const details = (await app.request(`${app.path}/activity/requests/${tools.reply}`, app.cookie)).value;
  assert.ok(details.model_usage.calls >= 1 && details.model_usage.models === 'fixture');
  assert.equal(app.processes().some(p => p.logs.includes(secret)), false, 'the key never appears in logs');
  await app.stop(executor);
  app.pass('Real Goose replies through the executor proxy and the controller gateway; the executor never holds the key; tokens are recorded per request');
});
