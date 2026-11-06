// The model gateway (core-agent CA3) with real Goose: executors hold no model key; the
// controller picks the assignment's model, adds the provider key from its own file,
// streams the answer back and records token use. The model is scripted; Goose, the
// controller, the executor's loopback proxy and worker control are real.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { once } from 'node:events';
import { join, resolve } from 'node:path';
import { createRequire } from 'node:module';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { fixtureProvider } from './fixtures/goose_provider.mjs';

const root = resolve(import.meta.dirname, '..');
const { chromium } = createRequire(join(root, 'web/package.json'))('playwright');

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

// MG1: people choose Cuckoo's catalog models; the controller picks the provider. Here the
// first route's provider is down, so the call moves to the next route before any answer
// reaches the agent; the executor serves any catalog model ("*").
test('a catalog model is served by its next route when the first provider is down', { skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 180000 }, async t => {
  const secret = 'provider-key-only-the-controller-reads';
  const provider = await fixtureProvider({ key: secret });
  let downCalls = 0;
  const down = createServer((request, response) => { downCalls += 1; request.resume(); response.writeHead(503).end('{"error":"overloaded"}'); }).listen(0, '127.0.0.1');
  await once(down, 'listening');
  t.after(() => down.close());
  const directory = await mkdtemp(join(root, '.tmp/model-catalog-'));
  const keyFile = join(directory, 'openai.key'), providers = join(directory, 'providers.json'), catalog = join(directory, 'catalog.json');
  await writeFile(keyFile, secret + '\n', { mode: 0o600 });
  await writeFile(providers, JSON.stringify([
    { provider: 'down', base_url: `http://127.0.0.1:${down.address().port}/v1`, key_file: keyFile },
    { provider: 'fixture-host', base_url: `${provider.url}/v1`, key_file: keyFile },
  ]), { mode: 0o600 });
  await writeFile(catalog, JSON.stringify({ models: [{ id: 'fixture', name: 'Fixture Large', tags: ['fast'], price: { input: 1, output: 2 },
    routes: [{ provider: 'down', model: 'down-fixture', priority: 1 }, { provider: 'fixture-host', model: 'fixture', priority: 2, cost: { input: 0.5, output: 1 } }] }] }));
  const app = await workerFixture(t, { mode: 'pooled', control: true, capacity: 1, workspaceLimit: 1, modelLimit: 1, leaseSeconds: 15, provider, agentProvider: '',
    apiEnv: { AYATI_MODEL_PROVIDERS_FILE: providers, AYATI_MODEL_CATALOG_FILE: catalog } });
  const offered = (await app.request(`${app.path}/models`, app.cookie)).value;
  assert.equal(offered.catalog_on, true);
  assert.deepEqual(offered.models.map(m => m.name), ['Fixture Large']);
  assert.equal(JSON.stringify(offered).includes('fixture-host'), false, 'people never see providers or routes');
  // The agent's Environment tab shows the catalog model by name, with no provider.
  const browser = await chromium.launch({ headless: true });
  t.after(() => browser.close());
  const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  const [name, value] = app.cookie.split('=');
  await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
  const page = await context.newPage();
  await page.goto(`${app.base}/w/${app.path.split('/').at(-1)}/?view=agents`);
  await page.getByRole('tab', { name: 'Environment', exact: true }).click();
  const picker = page.getByRole('group', { name: 'Model' });
  await picker.getByText('Fixture Large', { exact: true }).waitFor();
  assert.equal(await picker.getByRole('radio', { name: /Fixture Large/ }).isChecked(), true);
  assert.equal((await page.locator('[role=tabpanel]').textContent()).includes('fixture-host'), false);
  await page.screenshot({ path: join(app.data, 'model-catalog-picker.png') });
  await page.evaluate(() => { document.documentElement.dataset.theme = 'dark'; });
  await page.screenshot({ path: join(app.data, 'model-catalog-picker-dark.png') });
  await page.setViewportSize({ width: 390, height: 844 });
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
  await page.screenshot({ path: join(app.data, 'model-catalog-picker-mobile.png') });
  const executor = await app.startExecutor({ AYATI_MODEL_GATEWAY: '1', AYATI_MODEL: '*', OPENAI_API_KEY: undefined, OPENAI_HOST: undefined, AYATI_EXECUTOR_SLOTS: '1' });
  const job = await app.send('Say hello through the catalog');
  await app.done(job);
  assert.ok(downCalls >= 1, 'the first route was tried');
  assert.ok(provider.requests.length >= 1 && provider.requests.every(r => r.model === 'fixture'), "the second route's model name is sent");
  const row = app.sql("SELECT provider || ':' || model || ':' || provider_model || ':' || attempts FROM relay_model_usage WHERE agent IS NOT NULL ORDER BY id LIMIT 1");
  assert.equal(row, 'fixture-host:fixture:fixture:2', 'usage records who served it after how many tries');
  // MG2: the charge is tokens × the catalog price; the cost is at the route's cost.
  const [input, output, charge, cost, person] = app.sql("SELECT input_tokens || ':' || output_tokens || ':' || charge_micro || ':' || cost_micro || ':' || person FROM relay_model_usage WHERE agent IS NOT NULL ORDER BY id LIMIT 1").split(':');
  assert.ok(Number(input) > 0, 'tokens were counted');
  assert.equal(Number(charge), Number(input) * 1 + Number(output) * 2);
  assert.equal(Number(cost), Math.round(Number(input) * 0.5 + Number(output) * 1));
  const me = (await app.request('/api/identity/session', app.cookie)).value.user;
  assert.equal(person, me.id, 'the call is counted for the person who asked');
  const usage = (await app.request(`${app.path}/usage?days=7`, app.cookie)).value;
  assert.ok(usage.totals.charge_usd > 0 && usage.by_model[0].name === 'Fixture Large');
  assert.equal(JSON.stringify(usage).includes('cost'), false, 'our cost never reaches people');
  await page.setViewportSize({ width: 1280, height: 900 });
  await page.evaluate(() => { document.documentElement.dataset.theme = 'light'; });
  await page.goto(`${app.base}/w/${app.path.split('/').at(-1)}/?view=settings&section=usage`);
  await page.getByRole('region', { name: 'Workspace usage' }).getByText('Fixture Large', { exact: true }).waitFor();
  await page.screenshot({ path: join(app.data, 'model-usage.png'), fullPage: true });
  await app.stop(executor);

  // MG3: a used-up budget stops model calls at the gateway, even for work queued
  // before it ran out, and refuses new requests with the reason.
  const w = app.path.split('/').at(-1);
  await app.request(`${app.path}/model-budgets`, app.cookie, { agent: '', monthly_usd: 0.01, warn_percent: 80 });
  const queued = await app.send('Queued before the budget ran out');
  app.sql(`INSERT INTO relay_model_usage(workspace,assignment,agent,provider,model,input_tokens,output_tokens,charge_micro) VALUES('${w}','fixture','${app.profile.id}','fixture-host','fixture',0,0,10000)`);
  const asked = provider.requests.length;
  const again = await app.startExecutor({ AYATI_MODEL_GATEWAY: '1', AYATI_MODEL: '*', OPENAI_API_KEY: undefined, OPENAI_HOST: undefined, AYATI_EXECUTOR_SLOTS: '1' });
  await app.done(queued, 'failed');
  assert.equal(provider.requests.length, asked, 'no call reached a provider after the budget ran out');
  const refused = await app.request(`${app.path}/chats/${app.conversation}/messages`, app.cookie, { id: crypto.randomUUID(), content: 'Refused', agent: app.profile.id }, 400);
  assert.match(refused.value.error, /This workspace has used its monthly model budget/);
  const listed = (await app.request(`${app.path}/model-budgets`, app.cookie)).value.budgets[0];
  assert.equal(listed.state, 'reached');
  await page.goto(`${app.base}/w/${w}/?view=settings&section=usage`);
  await page.getByRole('region', { name: 'Monthly budgets' }).getByText(/Used up/).waitFor();
  await page.screenshot({ path: join(app.data, 'model-budget-reached.png'), fullPage: true });
  await app.stop(again);
  app.pass('A catalog model moves to its next route when the first provider is down; the executor serves any catalog model; usage records the provider, tries, charge, cost and person, and Settings shows the charges; a used-up budget stops calls at the gateway and refuses new requests');
});
