import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { mkdir } from 'node:fs/promises';
import { setup, root } from './fixtures/goose_app.mjs';
const { chromium } = createRequire(join(root, 'web/package.json'))('playwright');

test('setup guide requires explicit testing, protects history, and sanitizes failures', { timeout: 60000 }, async t => {
  const app = await setup(t, false, { AYATI_BROWSER: '0', AYATI_MEMORY: '0' });
  const initial = await app.state();
  const status = await app.request('/setup');
  assert.equal(status.can_test, true);
  assert.equal(status.provider, 'openai');
  assert(status.checks.every(check => check.ok));
  assert(!JSON.stringify(status).includes('fixture-not-a-secret'));
  assert.equal(app.provider.requests.length, 0);
  for (const body of [{ confirm: false }, {}, { confirm: true, model: 'other' }]) {
    const response = await fetch(app.base + '/setup/test', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
    assert(!response.ok);
  }
  const forbidden = await fetch(app.base + '/setup/test', { method: 'POST', headers: { 'Content-Type': 'application/json', Origin: 'https://untrusted.example' }, body: '{"confirm":true}' });
  assert.equal(forbidden.status, 403);
  assert.equal(app.provider.requests.length, 0);
  const browser = await chromium.launch(); t.after(() => browser.close());
  const page = await browser.newPage({ viewport: { width: 390, height: 844 } });
  const errors = []; page.on('pageerror', e => errors.push(e.message));
  await page.goto(app.base.replace('/api', '/?view=settings'));
  await page.getByText('Local checks passed.', { exact: false }).waitFor();
  const button = page.getByRole('button', { name: 'Test provider connection' });
  assert(await button.isDisabled());
  await page.getByRole('checkbox').check(); await button.click();
  await page.getByText('Provider returned text', { exact: false }).waitFor();
  assert(await button.isDisabled());
  assert.equal(app.provider.requests.length, 1);
  assert.deepEqual(app.provider.requests[0].messages, [{ role: 'user', content: 'Reply with OK.' }]);
  assert.equal(app.provider.requests[0].max_tokens, 64);
  assert.equal(app.provider.requests[0].tools, undefined);
  assert.deepEqual(await app.state(), initial);
  app.provider.setUnavailable(true);
  await page.getByRole('checkbox').check(); await button.click();
  await page.getByText('Provider returned HTTP 403', { exact: false }).waitFor();
  assert(!(await page.locator('body').innerText()).includes('PRIVATE_PROVIDER_FAILURE_BODY'));
  assert.equal(app.provider.requests.length, 2);
  assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
  await mkdir(join(root, '.test-runs/setup'), { recursive: true });
  await page.screenshot({ path: join(root, '.test-runs/setup/mobile.png'), fullPage: true });
  await page.setViewportSize({ width: 1440, height: 960 });
  await page.screenshot({ path: join(root, '.test-runs/setup/desktop.png'), fullPage: true });
  assert.deepEqual(errors, []);
  await app.stop(); app.configure({ OPENAI_API_KEY: '', AYATI_GOOSE_BIN: '/missing/goose' }); await app.start();
  const missing = await app.request('/setup');
  assert.equal(missing.can_test, false);
  assert.equal(missing.checks.find(c => c.label === 'Goose 1.54.0').ok, false);
  assert.equal(missing.checks.find(c => c.label === 'OPENAI_API_KEY').ok, false);
  const response = await fetch(app.base + '/setup/test', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: '{"confirm":true}' });
  assert.equal(response.status, 400);
  assert.equal(app.provider.requests.length, 2);
  await app.stop(); app.configure({ AYATI_EXECUTION: 'vm', AYATI_VM_MANIFEST: '/missing/vm.json' }); await app.start();
  const vm = await app.request('/setup');
  assert.equal(vm.execution, 'vm');
  assert.equal(vm.checks.find(c => c.label === 'VM manifest and files').ok, false);
  assert.equal(app.provider.requests.length, 2);
});

test('connection test rejects overlap and redirects without exposing provider output', { timeout: 20000 }, async t => {
  const { createServer } = await import('node:http');
  const { once } = await import('node:events');
  let release;
  let received;
  const arrival = new Promise(resolve => { received = resolve; });
  let mode = 'hold'; let count = 0;
  const server = createServer(async (req, res) => {
    count++;
    for await (const chunk of req) { /* Drain synthetic request. */ }
    if (mode === 'hold') {
      received(); await new Promise(resolve => { release = resolve; });
      res.setHeader('Content-Type', 'application/json');
      res.end('{"choices":[{"message":{"content":"OK"}}]}');
    } else if (mode === 'redirect') {
      res.writeHead(302, { Location: '/redirected' }); res.end();
    } else { res.end('PRIVATE_MALFORMED_PROVIDER_BODY'); }
  });
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  t.after(() => { release?.(); server.closeAllConnections(); server.close(); });
  const app = await setup(t, false, { AYATI_BROWSER: '0', AYATI_MEMORY: '0', OPENAI_HOST: `http://127.0.0.1:${server.address().port}` });
  const options = { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: '{"confirm":true}' };
  const first = fetch(app.base + '/setup/test', options);
  await arrival;
  const second = await fetch(app.base + '/setup/test', options);
  assert.equal(second.status, 400);
  assert.match(await second.text(), /already running/);
  assert.equal(count, 1);
  release(); assert((await first).ok);
  mode = 'redirect';
  const redirect = await fetch(app.base + '/setup/test', options);
  assert.equal(redirect.status, 400);
  assert.match(await redirect.text(), /HTTP 302/);
  assert.equal(count, 2);
  mode = 'malformed';
  const malformed = await fetch(app.base + '/setup/test', options);
  assert.equal(malformed.status, 400);
  assert(!(await malformed.text()).includes('PRIVATE_MALFORMED_PROVIDER_BODY'));
});
