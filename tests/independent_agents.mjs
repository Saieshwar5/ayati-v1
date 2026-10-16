import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { mkdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { setup, until, vmManifest, root } from './fixtures/goose_app.mjs';
import { browserSite } from './fixtures/browser_site.mjs';

const memorySearch = 'memory ' + JSON.stringify({ tool: 'search', arguments: { query: 'report' } });
function client(app, id) {
  async function raw(path, body, headers = {}) {
    return fetch(app.base + path, { headers: { 'X-Ayati-Agent': id, 'Content-Type': 'application/json', Connection: 'close', ...headers },
      ...(body === undefined ? {} : { method: 'POST', body: JSON.stringify(body) }) });
  }
  async function request(path, body) {
    const response = await raw(path, body);
    assert.ok(response.ok, await response.clone().text()); return response.json();
  }
  async function finish(id) {
    return until(async () => {
      const state = await request('/state');
      for (const permission of state.permissions ?? []) if (permission.task_id === id) await request(`/permissions/${permission.id}`, { allow: true });
      const task = state.tasks.find(task => task.id === id);
      if (['completed', 'failed', 'stopped', 'interrupted'].includes(task?.status)) return { task, state,
        output: state.messages.filter(m => m.task_id === id && m.role === 'assistant').map(m => m.content).join('') };
    }, 'Selected agent did not finish', 60000);
  }
  return { raw, request, finish, async run(content) { return finish((await request('/messages', { content })).id); } };
}

test('owner creates a second real guest in the UI; chat, memory, approvals, browser and Stop stay separate after restart', { timeout: 360000, skip: !vmManifest }, async t => {
  const site = await browserSite(); t.after(site.close);
  const app = await setup(t, false, { AYATI_BROWSER: '1', AYATI_MEMORY: '1', AYATI_BROWSER_FIXTURE_PORT: String(site.port) });
  const first = client(app, 'local-agent');
  const { chromium } = createRequire(join(root, 'web/package.json'))('playwright');
  const browser = await chromium.launch(); t.after(() => browser.close());
  const page = await browser.newPage({ viewport: { width: 1280, height: 850 } });
  await page.goto(app.base.replace('/api', ''));
  await page.locator('#new-agent').click();
  await page.locator('[name=name]').fill('Operations');
  await page.locator('[name=role]').fill('Prepare daily operations reports. Keep customer records separate.');
  await page.locator('#setup-submit').click();
  const created = await until(async () => (await app.request('/agents')).agents.find(a => a.profile.name === 'Operations')?.profile, 'No durable agent receipt');
  assert.equal(created.state, 'preparing');
  const duplicateInput = { request_id: created.id, name: created.name, role: created.role, model: created.model, memory: true, browser: true };
  assert.equal((await app.request('/agents', duplicateInput)).id, created.id, 'Lost acknowledgement must not create another VM');
  assert.equal((await first.raw('/agents', { ...duplicateInput, name: 'Conflict' })).status, 400);
  assert.equal((await first.raw('/agents', { ...duplicateInput, request_id: randomUUID() })).status, 400, 'Only one concurrent preparation');
  assert.equal((await client(app, created.id).raw('/messages', { content: 'too early' })).status, 404);
  await first.request('/memory', { key: 'report.style', content: 'First agent reports use Telugu' });
  const initial = await first.run(memorySearch);
  assert.equal(initial.task.status, 'completed', initial.task.detail); assert.match(initial.output, /Telugu/);
  const ready = await until(async () => {
    const profile = (await app.request('/agents')).agents.find(a => a.profile.id === created.id).profile;
    assert.notEqual(profile.state, 'failed', profile.detail);
    return profile.state === 'ready' && profile;
  }, 'VM setup did not complete', 180000);
  const second = client(app, ready.id);
  assert.equal((await second.request('/state')).messages.length, 0);
  assert.equal((await second.request('/memory')).memories.length, 0);
  const secondFact = await second.request('/memory', { key: 'report.style', content: 'Second agent reports use English' });
  const secondRead = await second.run(memorySearch);
  assert.equal(secondRead.task.status, 'completed', secondRead.task.detail);
  assert.match(secondRead.output, /English/); assert.doesNotMatch(secondRead.output, /Telugu/);
  assert.ok(app.provider.requests.some(r => JSON.stringify(r.messages).includes(ready.role)), 'Configured role reaches Goose');
  const tools = app.provider.requests.at(-1).tools.map(tool => tool.function.name);
  assert.ok(tools.includes('ayati_memory__memory_search') && tools.includes('ayati_browser__agent_browser_open'));
  assert.ok(!tools.some(name => /shell|developer|recall/.test(name)));
  assert.equal((await first.raw(`/memory/${secondFact.id}/forget`, {})).status, 400);
  assert.equal((await first.raw(`/tasks/${secondRead.task.id}/stop`, {})).status, 400);
  assert.equal((await first.raw(`/tasks/${secondRead.task.id}/actions`)).status, 400);
  assert.equal((await first.raw(`/files/${secondFact.id}`)).status, 400);
  assert.equal((await client(app, 'missing-agent').raw('/state')).status, 404);
  assert.equal((await first.raw(`/events?agent=${ready.id}`)).status, 404, 'Conflicting selectors fail closed');
  assert.equal((await first.raw(`/events?agent=local-agent&agent=${ready.id}`)).status, 404);
  const open = await second.run('browser ' + JSON.stringify({ tool: 'open', arguments: { url: 'http://fixture.ayati.invalid' } }));
  assert.equal(open.task.status, 'completed', open.task.detail); assert.match(open.output, /Ayati browser fixture/);
  const aVm = await first.request('/vm/status'), bVm = await second.request('/vm/status');
  assert.equal(aVm.agent_id, 'local-agent'); assert.equal(bVm.agent_id, ready.id); assert.notEqual(aVm.pid, bVm.pid);
  const aManifest = JSON.parse(await readFile(vmManifest, 'utf8'));
  const bManifest = JSON.parse(await readFile(join(app.data, 'vms', ready.attempt, 'vm.json'), 'utf8'));
  assert.notEqual(aManifest.directory, bManifest.directory);
  assert.notEqual(await readFile(join(aManifest.directory, 'client_key'), 'utf8'), await readFile(join(bManifest.directory, 'client_key'), 'utf8'));
  const controller = randomUUID();
  await first.request('/browser/control', { op: 'take', token: controller });
  assert.equal((await second.raw('/browser/control', { op: 'resume', token: controller })).status, 400);
  assert.equal((await first.request('/browser/status')).owner, true);
  assert.equal((await second.request('/browser/status')).owner, false);
  await first.request('/browser/control', { op: 'resume', token: controller });
  const waiting = await second.request('/messages', { content: memorySearch });
  const permission = await until(async () => (await second.request('/state')).permissions.find(p => p.task_id === waiting.id), 'No selected agent approval');
  assert.equal((await first.raw(`/permissions/${permission.id}`, { allow: true })).status, 400);
  assert.equal((await first.request('/state')).permissions.length, 0);
  await second.finish(waiting.id);
  const stalledA = await first.request('/messages', { content: 'stall' });
  const stalledB = await second.request('/messages', { content: 'stall' });
  await until(async () => (await first.request('/state')).tasks.find(t => t.id === stalledA.id)?.status === 'running'
    && (await second.request('/state')).tasks.find(t => t.id === stalledB.id)?.status === 'running', 'Both agents must run independently');
  await first.request(`/tasks/${stalledA.id}/stop`, {});
  assert.equal((await first.finish(stalledA.id)).task.status, 'stopped');
  assert.equal((await second.request('/state')).tasks.find(t => t.id === stalledB.id).status, 'running');
  await second.request(`/tasks/${stalledB.id}/stop`, {});
  await second.finish(stalledB.id);
  // Full-page switching gives every request and stream an immutable selection.
  await page.locator('#input').fill('Unsent first-agent draft');
  await page.getByRole('link', { name: 'Operations', exact: true }).click();
  await page.waitForURL(`**/?agent=${ready.id}`);
  await until(async () => (await page.locator('.identity strong').textContent()) === 'Operations', 'Selected name missing');
  assert.equal(await page.locator('#input').inputValue(), '');
  assert.doesNotMatch(await page.locator('#conversation').textContent(), /First agent reports use Telugu/);
  await page.locator('#input').fill('Unsent second-agent draft');
  await page.getByRole('link', { name: 'Ayati', exact: true }).click();
  await page.waitForURL('**/?agent=local-agent');
  assert.equal(await page.locator('#input').inputValue(), 'Unsent first-agent draft');
  await page.getByRole('link', { name: 'Operations', exact: true }).click();
  await page.waitForURL(`**/?agent=${ready.id}`);
  assert.equal(await page.locator('#input').inputValue(), 'Unsent second-agent draft');
  await page.setViewportSize({ width: 390, height: 800 });
  assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
  assert.ok(await page.locator('#composer').evaluate(el => el.getBoundingClientRect().bottom <= innerHeight));
  await mkdir(join(root, '.test-runs/agents'), { recursive: true });
  await page.screenshot({ path: join(root, '.test-runs/agents/mobile.png') });
  await page.setViewportSize({ width: 1280, height: 850 });
  await page.screenshot({ path: join(root, '.test-runs/agents/desktop.png') });
  await page.close();
  await app.stop(); await app.start();
  assert.equal((await second.request('/memory')).memories[0].content, secondFact.content);
  assert.ok((await second.request('/state')).messages.some(m => m.task_id === secondRead.task.id));
  assert.ok(!(await first.request('/state')).messages.some(m => m.task_id === secondRead.task.id));
  const again = await second.run(memorySearch);
  assert.equal(again.task.status, 'completed', again.task.detail); assert.match(again.output, /English/); assert.doesNotMatch(again.output, /Telugu/);
  assert.equal((await first.raw('/agents', duplicateInput, { Origin: 'https://untrusted.example' })).status, 403);
});

test('interrupted setup is durable, fails closed after crash and can be explicitly retried', { timeout: 240000, skip: !vmManifest }, async t => {
  const app = await setup(t, false);
  const input = { request_id: randomUUID(), name: 'Recovery agent', role: 'Prepare summaries', model: 'fixture', memory: false, browser: false };
  const created = await app.request('/agents', input);
  await app.crash(); await app.start();
  const failed = (await app.request('/agents')).agents.find(a => a.profile.id === created.id).profile;
  assert.equal(failed.state, 'failed');
  assert.equal((await client(app, created.id).raw('/state')).status, 404);
  assert.equal((await app.request('/agents', input)).id, created.id);
  const retry = await app.request(`/agents/${created.id}/retry`, {});
  assert.notEqual(retry.attempt, created.attempt);
  const ready = await until(async () => {
    const profile = (await app.request('/agents')).agents.find(a => a.profile.id === created.id).profile;
    assert.notEqual(profile.state, 'failed', profile.detail); return profile.state === 'ready';
  }, 'Retry failed to provision', 180000);
  assert.ok(ready);
  const run = await client(app, created.id).run('hello');
  assert.equal(run.task.status, 'completed', run.task.detail);
});
