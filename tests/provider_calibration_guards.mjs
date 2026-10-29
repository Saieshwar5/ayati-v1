import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { once } from 'node:events';
import { mkdtemp, writeFile, chmod, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync, spawnSync } from 'node:child_process';
import { Budget } from './calibration/budget.mjs';
import { microDollars, readPrivateKey, scriptedPlan, validatePlan } from './calibration/plan.mjs';
import { StreamObserver } from './calibration/sse.mjs';
import { calibrationProxy, upstreamBody } from './calibration/proxy.mjs';
import { scriptedUpstream } from './calibration/scripted.mjs';
import { until } from './fixtures/polling.mjs';

const marker = 'CAL_aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee';
const input = { model: 'fixture', stream: true, messages: [{ role: 'user', content: marker }] };
const post = (proxy, body = input, signal, key = proxy.key) => fetch(proxy.url + '/v1/chat/completions', {
  method: 'POST', headers: { Authorization: 'Bearer ' + key, 'Content-Type': 'application/json' },
  body: JSON.stringify(body), signal,
});

test('CLI preflight stays scripted with ambient keys and refuses missing live authorization', () => {
  const image = 'sha256:' + 'a'.repeat(64);
  const command = ['tools/calibrate-provider.mjs', '--image', image];
  const output = execFileSync(process.execPath, [...command, '--check'], { encoding: 'utf8',
    env: { ...process.env, OPENAI_API_KEY: 'PRIVATE_AMBIENT_KEY', AYATI_CALIBRATION_LIVE: '1' } });
  assert.equal(JSON.parse(output).mode, 'scripted');
  assert.equal(output.includes('PRIVATE_AMBIENT_KEY'), false);
  const refused = spawnSync(process.execPath, [...command, '--live'], { encoding: 'utf8' });
  assert.equal(refused.status, 1);
  assert.equal(refused.stdout.includes('Retained provider calibration'), false);
});

test('plans require an explicit live provider, current rates and bounded fields', () => {
  const plan = scriptedPlan();
  validatePlan(plan);
  assert.throws(() => validatePlan(plan, 'false'), /explicit boolean/);
  for (const changes of [{ model: 'REPLACE' }, { concurrency: 0 }, { max_output_tokens: 0 },
    { budget_usd: 'NaN' }, { input_usd_per_million: '0' }, { endpoint: 'https://attacker.test' }]) {
    assert.throws(() => validatePlan({ ...plan, ...changes }));
  }
  assert.throws(() => validatePlan(plan, true));
  assert.throws(() => validatePlan({ ...plan, provider: 'openai' }));
  const live = { ...plan, provider: 'openai', model: 'explicit-model', pricing_source: 'https://openai.com/api/pricing/' };
  validatePlan(live, true);
  assert.throws(() => validatePlan({ ...live, pricing_checked: '2000-01-01' }, true));
  assert.throws(() => validatePlan({ ...live, pricing_source: 'https://openai.com.attacker.test/' }, true));
  assert.equal(microDollars('1.000001'), 1000001n);
});

test('private key permissions are enforced without including key values in errors', async t => {
  const dir = await mkdtemp(join(tmpdir(), 'calibration-key-'));
  t.after(() => rm(dir, { recursive: true }));
  const file = join(dir, 'key');
  await writeFile(file, 'PRIVATE_KEY_SENTINEL\n', { mode: 0o600 });
  assert.equal(readPrivateKey(file), 'PRIVATE_KEY_SENTINEL');
  await chmod(file, 0o644);
  assert.throws(() => readPrivateKey(file), /owner-only/);
});

test('reservations are atomic, include every retry and never refund uncertain calls', () => {
  const budget = new Budget({ ...scriptedPlan(), concurrency: 2, budget_usd: '1.00' });
  const first = budget.reserve(marker, 10), second = budget.reserve(marker, 10);
  assert.throws(() => budget.reserve(marker, 10), /concurrency/);
  budget.finish(first, 'client_cancelled'); budget.finish(second, 'upstream_failed');
  assert.equal(budget.report().reserved_micro_usd, (budget.perCall * 2n).toString());
  while (budget.reserved + budget.perCall <= budget.limit) budget.finish(budget.reserve(marker, 10), 'usage_missing');
  assert.throws(() => budget.reserve(marker, 10), /budget/);
  const calls = new Budget({ ...scriptedPlan(), max_requests: 7, budget_usd: '10' });
  for (let i = 0; i < 7; i++) calls.finish(calls.reserve(marker, 10), 'complete');
  assert.throws(() => calls.reserve(marker, 10), /request_limit/);
});

test('fragmented SSE distinguishes reasoning from visible text and validates usage', () => {
  const budget = new Budget(scriptedPlan()), row = budget.reserve(marker, 10);
  const observer = new StreamObserver(row, budget);
  const frame = value => 'data: ' + JSON.stringify(value) + '\r\n\r\n';
  observer.push(Buffer.from(frame({ choices: [{ delta: { reasoning_content: 'PRIVATE' } }] })));
  assert.equal(row.first_text_ms, undefined);
  const rest = Buffer.from(frame({ choices: [{ delta: { content: '🦀' }, finish_reason: 'stop' }] })
    + frame({ choices: [], usage: { prompt_tokens: 100, completion_tokens: 3 } }) + 'data: [DONE]\r\n\r\n');
  for (const byte of rest) observer.push(Buffer.from([byte]));
  observer.finish();
  assert.ok(row.first_text_ms); assert.equal(row.usage.completion_tokens, 3);
  assert.throws(() => observer.push(Buffer.from('data: {}\n\n')), /after stream/);
  const incomplete = new StreamObserver({}, budget);
  incomplete.push(Buffer.from('data: {}\n\n')); assert.throws(() => incomplete.finish(), /Incomplete/);
  const over = budget.reserve(marker, 10);
  assert.throws(() => budget.usage(over, { prompt_tokens: 999999, completion_tokens: 1 }), /exceeded/);
  assert.throws(() => budget.reserve(marker, 10), /exceeded/);
  const oversized = new StreamObserver({}, budget);
  assert.throws(() => oversized.push(Buffer.from('data: ' + 'x'.repeat(65536) + '\n\n')), /frame exceeds/);
});

test('provider output limits override harness fields and omit unsupported paid features', () => {
  const body = upstreamBody({ ...input, max_completion_tokens: 99999, tools: [{}], service_tier: 'priority' },
    { ...scriptedPlan(), provider: 'openai' });
  assert.equal(body.max_completion_tokens, 256); assert.equal(body.max_tokens, undefined);
  assert.equal(body.tools, undefined); assert.equal(body.service_tier, undefined);
  assert.equal(upstreamBody(input, { ...scriptedPlan(), provider: 'fireworks' }).max_tokens, 256);
});

test('proxy forwards only scoped text requests with enforced model/token limits', async t => {
  const upstream = await scriptedUpstream(); t.after(() => upstream.close());
  const proxy = await calibrationProxy({ plan: scriptedPlan(), scriptedUrl: upstream.url }); t.after(() => proxy.close());
  assert.equal((await post(proxy, input, undefined, 'foreign')).status, 403);
  assert.equal((await post(proxy, { ...input, model: 'other' })).status, 400);
  assert.equal((await post(proxy, { ...input, tools: [{}] })).status, 400);
  assert.equal((await post(proxy, { ...input, messages: [{ role: 'user', content: 'unmarked' }] })).status, 400);
  assert.equal(upstream.observed.length, 0);
  const response = await post(proxy, { ...input, max_tokens: 1000000 });
  assert.equal(response.status, 200); await response.text();
  await until(() => proxy.snapshot().active === 0, 'Proxy did not finish');
  assert.equal(upstream.observed.length, 1);
  assert.equal(upstream.observed[0].max_tokens, 256);
  assert.equal(upstream.observed[0].include_usage, true);
  assert.equal(proxy.snapshot().requests[0].state, 'complete');
  assert.equal(JSON.stringify(proxy.snapshot()).includes('PRIVATE_CALIBRATION_REASONING'), false);
});

test('cancellation aborts the upstream and retains its entire reservation', async t => {
  const upstream = await scriptedUpstream(); t.after(() => upstream.close());
  const proxy = await calibrationProxy({ plan: scriptedPlan(), scriptedUrl: upstream.url }); t.after(() => proxy.close());
  const abort = new AbortController();
  const response = await post(proxy, { ...input, messages: [{ role: 'user', content: marker + ' CAL_CANCEL' }] }, abort.signal);
  await response.body.getReader().read(); abort.abort();
  await until(() => proxy.snapshot().active === 0, 'Abort did not settle');
  assert.equal(proxy.snapshot().requests[0].state, 'client_cancelled');
  assert.equal(proxy.snapshot().unknown_usage_requests, 1);
  assert.ok(BigInt(proxy.snapshot().reserved_micro_usd) > 0n);
});

test('concurrent requests cannot forward beyond the declared active limit', async t => {
  const upstream = await scriptedUpstream(); t.after(() => upstream.close());
  const proxy = await calibrationProxy({ plan: { ...scriptedPlan(), concurrency: 2 }, scriptedUrl: upstream.url });
  t.after(() => proxy.close());
  const aborts = [new AbortController(), new AbortController()];
  const held = { ...input, messages: [{ role: 'user', content: marker + ' CAL_CANCEL' }] };
  const responses = await Promise.all(aborts.map(abort => post(proxy, held, abort.signal)));
  assert.equal((await post(proxy)).status, 403);
  assert.equal(upstream.observed.length, 2);
  for (const abort of aborts) abort.abort();
  for (const response of responses) await response.body.cancel().catch(() => {});
  await until(() => proxy.snapshot().active === 0, 'Held streams did not settle');
  assert.equal(proxy.snapshot().denied[0].reason, 'concurrency_limit');
});

test('the request deadline closes a stalled stream without refunding its allowance', { timeout: 15000 }, async t => {
  const upstream = createServer(async (request, response) => {
    for await (const chunk of request) { /* Drain the synthetic request. */ }
    response.writeHead(200, { 'Content-Type': 'text/event-stream' });
    response.write('data: {"choices":[{"delta":{"role":"assistant"}}]}\n\n');
  });
  upstream.listen(0, '127.0.0.1'); await once(upstream, 'listening');
  t.after(() => new Promise(resolve => upstream.close(resolve)));
  const proxy = await calibrationProxy({ plan: { ...scriptedPlan(), timeout_seconds: 10 },
    scriptedUrl: `http://127.0.0.1:${upstream.address().port}` });
  t.after(() => proxy.close());
  const response = await post(proxy);
  await assert.rejects(() => response.text());
  await until(() => proxy.snapshot().active === 0, 'Stalled request remained active');
  assert.equal(proxy.snapshot().requests[0].state, 'timeout');
  assert.equal(proxy.snapshot().unknown_usage_requests, 1);
  assert.ok(BigInt(proxy.snapshot().reserved_micro_usd) > 0n);
});

test('redirects and provider error bodies are never forwarded or retried', async t => {
  let forwarded = 0;
  const target = createServer((request, response) => { forwarded++; response.end('PRIVATE_ERROR_BODY'); });
  target.listen(0, '127.0.0.1'); await once(target, 'listening');
  t.after(() => new Promise(resolve => target.close(resolve)));
  const upstream = createServer((request, response) => {
    response.writeHead(302, { Location: `http://127.0.0.1:${target.address().port}/leak` }); response.end();
  });
  upstream.listen(0, '127.0.0.1'); await once(upstream, 'listening');
  t.after(() => new Promise(resolve => upstream.close(resolve)));
  const proxy = await calibrationProxy({ plan: scriptedPlan(), scriptedUrl: `http://127.0.0.1:${upstream.address().port}` });
  t.after(() => proxy.close());
  const response = await post(proxy); assert.equal(response.status, 502);
  assert.equal((await response.text()).includes('PRIVATE_ERROR_BODY'), false);
  assert.equal(forwarded, 0); assert.equal(proxy.snapshot().requests.length, 1);
});

test('provider errors, missing usage and exhausted allowance stay explicit', async t => {
  let calls = 0;
  const upstream = createServer(async (request, response) => {
    for await (const chunk of request) { /* Drain the synthetic request. */ }
    calls++;
    if (calls === 1) { response.writeHead(429); response.end('PRIVATE_UPSTREAM_ERROR'); return; }
    response.writeHead(200, { 'Content-Type': 'text/event-stream' });
    response.end('data: {"choices":[{"delta":{"content":"ok"}}]}\n\ndata: [DONE]\n\n');
  });
  upstream.listen(0, '127.0.0.1'); await once(upstream, 'listening');
  t.after(() => new Promise(resolve => upstream.close(resolve)));
  const plan = { ...scriptedPlan(), budget_usd: '1' };
  const proxy = await calibrationProxy({ plan, scriptedUrl: `http://127.0.0.1:${upstream.address().port}` });
  t.after(() => proxy.close());
  const error = await post(proxy); assert.equal(error.status, 502);
  assert.equal((await error.text()).includes('PRIVATE_UPSTREAM_ERROR'), false);
  for (let i = 0; i < 6; i++) { const response = await post(proxy); assert.equal(response.status, 200); await response.text(); }
  const denied = await post(proxy); assert.equal(denied.status, 403); await denied.text();
  assert.equal(calls, 7); assert.equal(proxy.snapshot().unknown_usage_requests, 7);
  assert.equal(proxy.snapshot().requests[1].state, 'usage_missing');
  assert.equal(proxy.snapshot().denied[0].reason, 'budget_limit');
});

test('a fragmented SSE error cannot expose its private body to the harness', async t => {
  const upstream = createServer(async (request, response) => {
    for await (const chunk of request) { /* Drain the synthetic request. */ }
    response.writeHead(200, { 'Content-Type': 'text/event-stream' });
    response.write('data: {"error":{"message":"PRIVATE_');
    setTimeout(() => response.end('STREAM_ERROR"}}\n\n'), 20);
  });
  upstream.listen(0, '127.0.0.1'); await once(upstream, 'listening');
  t.after(() => new Promise(resolve => upstream.close(resolve)));
  const proxy = await calibrationProxy({ plan: scriptedPlan(), scriptedUrl: `http://127.0.0.1:${upstream.address().port}` });
  t.after(() => proxy.close());
  let returned = '';
  try { const response = await post(proxy); returned = await response.text(); } catch { /* Closed stream is expected. */ }
  assert.equal(returned.includes('PRIVATE_'), false);
  await until(() => proxy.snapshot().active === 0, 'Error stream remained active');
  assert.equal(proxy.snapshot().requests[0].state, 'upstream_failed');
});
