import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { mkdir, writeFile } from 'node:fs/promises';
import { setup,until,root,vmManifest } from './fixtures/goose_app.mjs';

async function run(app, command, allow = true) {
  const input = await app.submit(command);
  return finish(app, input.id, allow);
}
async function finish(app, id, allow = true) {
  const timer = setInterval(async () => {
    try { for (const p of (await app.state()).permissions ?? []) if (p.task_id === id) await app.request(`/permissions/${p.id}`, { allow }); } catch { /* Permission can expire during polling. */ }
  }, 30);
  try { return await app.done(id); } finally { clearInterval(timer); }
}
function output(result) { return result.state.messages.filter(m => m.task_id === result.task.id && m.role === 'assistant').map(m => m.content).join(''); }
const command = (tool, args) => 'memory ' + JSON.stringify({ tool, arguments: args });

test('real Goose retrieves scoped sourced memory and corrections/forgetting reset stale sessions', { timeout: 180000 }, async t => {
  const app = await setup(t, false, { AYATI_MEMORY: '1' });
  const first = await app.request('/memory', { key: 'report.language', content: 'Reports should use Telugu' });
  const read = await run(app, command('search', { query: 'report language' }));
  assert.equal(read.task.status, 'completed', read.task.detail);
  assert.match(output(read), /Telugu/);
  assert.match(output(read), /owner_note/);
  const tools = app.provider.requests.at(-1).tools.map(t => t.function.name);
  assert.deepEqual(tools.sort(), ['ayati_memory__memory_remember', 'ayati_memory__memory_search', 'ayati_memory__memory_source']);
  const before = app.provider.requests.length;
  await app.request('/memory', { key: 'report.language', content: 'Reports should use English', replaces: first.id });
  const corrected = await run(app, command('search', { query: 'report' }));
  assert.equal(corrected.task.status, 'completed', corrected.task.detail);
  assert.match(output(corrected), /English/); assert.doesNotMatch(output(corrected), /Telugu/);
  assert.doesNotMatch(JSON.stringify(app.provider.requests[before].messages), /Telugu/);
  const current = (await app.request('/memory')).memories[0];
  await app.request(`/memory/${current.id}/forget`, {});
  await app.stop(); await app.start();
  const forgotten = await run(app, command('search', { query: 'report' }));
  assert.equal(forgotten.task.status, 'completed', forgotten.task.detail);
  assert.doesNotMatch(output(forgotten), /Telugu|English/);
  assert.equal((await app.request('/memory')).memories.length, 0);
  assert.ok((await app.state()).messages.some(m => m.content.includes('Telugu')), 'Original owner history remains inspectable');
});

test('real Goose source/remember tools require approval, preserve provenance and reject foreign sources', { timeout: 180000 }, async t => {
  const app = await setup(t, false, { AYATI_MEMORY: '1' });
  const input = 'memory ' + JSON.stringify({ steps: [{ tool: 'source', arguments: {} }, { tool: 'remember', useSource: true, arguments: { key: 'tea.preference', content: 'Prefers green tea', quote: 'green tea' } }] });
  const result = await run(app, input);
  assert.equal(result.task.status, 'completed', result.task.detail);
  assert.match(output(result), /tea.preference/);
  const saved = (await app.request('/memory')).memories[0];
  assert.equal(saved.source_kind, 'owner_message'); assert.equal(saved.quote, 'green tea');
  const forged = await run(app, command('remember', { key: 'forged', content: 'Other data', source_id: saved.source_id, quote: 'green tea' }));
  assert.equal(forged.task.status, 'completed', forged.task.detail);
  assert.match(output(forged), /denied|unavailable/);
  const denied = await run(app, command('search', { query: 'tea' }), false);
  assert.doesNotMatch(output(denied), /Prefers green tea/);
  assert.equal((await app.request('/memory')).memories.length, 1);
});

test('owner memory edit interrupts active work and the web panel saves, corrects and forgets safely', { timeout: 180000 }, async t => {
  const app = await setup(t, false, { AYATI_MEMORY: '1' });
  const require = createRequire(join(root, 'web/package.json'));
  const { chromium } = require('playwright');
  const browser = await chromium.launch({ headless: true }); t.after(() => browser.close());
  const page = await browser.newPage({ viewport: { width: 1100, height: 800 } });
  await page.goto(app.base.replace('/api', '/?agent=local-agent'));
  await page.locator('#memory-button').click();
  await page.locator('#memory-note [name=key]').fill('report.time');
  await page.locator('#memory-note [name=content]').fill('Daily report at 6 PM');
  await page.getByRole('button', { name: 'Save fact', exact: true }).click();
  await page.getByRole('button', { name: 'Correct', exact: true }).waitFor();
  await page.getByRole('button', { name: 'Correct', exact: true }).click();
  await page.locator('#memory-note [name=content]').fill('Daily report at 7 PM');
  await page.getByRole('button', { name: 'Save fact', exact: true }).click();
  await until(async () => (await app.request('/memory')).memories[0]?.content === 'Daily report at 7 PM', 'Correction failed');
  const job = await app.submit('stall');
  await until(() => app.provider.requests.some(r => JSON.stringify(r.messages).includes('stall')), 'No active model request');
  const queued = await app.submit(command('search', { query: 'report' }));
  await page.getByRole('button', { name: 'Forget', exact: true }).click();
  assert.equal((await app.done(job.id)).task.status, 'stopped');
  const afterForget = await finish(app, queued.id);
  assert.equal(afterForget.task.status, 'completed', afterForget.task.detail);
  assert.doesNotMatch(output(afterForget), /Daily report at [67] PM/);
  await until(async () => !(await app.request('/memory')).memories.length, 'Forget failed');
  await page.setViewportSize({ width: 390, height: 800 });
  await mkdir(join(root, '.test-runs/memory'), { recursive: true });
  await page.screenshot({ path: join(root, '.test-runs/memory/owner-panel.png') });
  assert.ok(await page.locator('#memory-panel').evaluate(el => el.getBoundingClientRect().right <= window.innerWidth));
  const response = await fetch(app.base + '/memory', { headers: { Origin: 'https://untrusted.example' } });
  assert.equal(response.status, 403);
  const local = await fetch(app.base + '/memory'); assert.equal(local.headers.get('cache-control'), 'no-store');
});


test('real Goose compacts working context and scoped memory remains retrievable afterward', { timeout: 180000, skip: !!vmManifest }, async t => {
  const app = await setup(t, false, { AYATI_MEMORY: '1' });
  await app.request('/memory', { key: 'report.language', content: 'Daily report language is Telugu' });
  await mkdir(join(app.data, 'goose/config'), { recursive: true });
  await writeFile(join(app.data, 'goose/config/config.yaml'), 'GOOSE_CONTEXT_LIMIT: 4000\nGOOSE_AUTO_COMPACT_THRESHOLD: 0.5\n');
  const long = await run(app, 'Continue preparing the daily owner report. ' + 'Synthetic old working detail. '.repeat(400));
  assert.equal(long.task.status, 'completed', long.task.detail);
  const next = await run(app, command('search', { query: 'report' }));
  assert.equal(next.task.status, 'completed', next.task.detail);
  assert.match(output(next), /Telugu/);
  const summaries = app.provider.requests.filter(r => JSON.stringify(r.messages).includes('Please summarize the conversation history provided in the system prompt.'));
  assert.ok(summaries.length > 0, 'The pinned Goose must actually request compaction');
  assert.ok(app.provider.requests.some(r => JSON.stringify(r.messages).includes('COMPACTION_FIXTURE_SUMMARY')), 'Compacted summary must be used in a later request');
  console.log('Memory fixture observations:', JSON.stringify({ providerCalls: app.provider.requests.length, compactionCalls: summaries.length, retrievalResultBytes: Buffer.byteLength(output(next)), paidInference: false }));
});
