import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { once } from 'node:events';
import { randomBytes } from 'node:crypto';
import { writeFileSync } from 'node:fs';
import { Budget } from './budget.mjs';
import { endpoints, readPrivateKey, validatePlan } from './plan.mjs';
import { StreamObserver } from './sse.mjs';

function boundedMessages(input, plan) {
  assert.equal(input.model, plan.model, 'Wrong model');
  assert.equal(input.stream, true, 'Streaming required');
  assert.ok(!input.tools?.length && !input.functions?.length && (input.n ?? 1) === 1,
    'Tool-free single completion required');
  assert.ok(Array.isArray(input.messages) && input.messages.length > 0 && input.messages.length <= 128);
  for (const message of input.messages) {
    assert.ok(['system', 'developer', 'user', 'assistant'].includes(message.role));
    assert.ok(!message.tool_calls?.length);
    assert.ok(typeof message.content === 'string' || (Array.isArray(message.content)
      && message.content.every(part => part.type === 'text' && typeof part.text === 'string')),
    'Only text input is supported');
  }
  const text = JSON.stringify(input.messages);
  const size = Buffer.byteLength(text);
  assert.ok(size <= plan.max_prompt_bytes, 'Prompt exceeds limit');
  const markers = [...new Set(text.match(/CAL_[a-f0-9-]{36}/g) ?? [])];
  assert.equal(markers.length, 1, 'Missing or mixed synthetic request markers');
  return { marker: markers[0], size };
}

export function upstreamBody(input, plan) {
  return { model: plan.model, messages: input.messages, stream: true,
    stream_options: { include_usage: true }, n: 1,
    [plan.provider === 'openai' ? 'max_completion_tokens' : 'max_tokens']: plan.max_output_tokens };
}

export async function calibrationProxy({ plan, live = false, keyFile, scriptedUrl, reportFile }) {
  validatePlan(plan, live);
  assert.ok(!live || !scriptedUrl, 'A live run cannot override its fixed endpoint');
  assert.ok(!live || process.env.NODE_TLS_REJECT_UNAUTHORIZED !== '0', 'TLS verification is required');
  let endpoint = endpoints[plan.provider];
  if (!live) {
    const url = new URL(scriptedUrl);
    assert.equal(url.protocol, 'http:');
    assert.equal(url.hostname, '127.0.0.1');
    assert.equal(url.pathname, '/');
    assert.ok(!url.username && !url.password && !url.search && !url.hash);
    endpoint = new URL('/v1/chat/completions', url).href;
  }
  const secret = live ? readPrivateKey(keyFile) : 'scripted-upstream-key';
  const key = randomBytes(32).toString('base64url');
  const budget = new Budget(plan);
  const aborts = new Set();
  const tasks = new Set();
  const snapshot = () => ({ version: 1, mode: live ? 'live' : 'scripted', provider: plan.provider,
    model: plan.model, endpoint, plan, ...budget.report() });
  const save = () => { if (reportFile) writeFileSync(reportFile, JSON.stringify(snapshot(), null, 2) + '\n', { mode: 0o600 }); };
  save();
  function fail(response, status, code) {
    if (response.headersSent) { response.destroy(); return; }
    response.writeHead(status, { 'Content-Type': 'application/json' });
    response.end(JSON.stringify({ error: { message: 'Calibration request refused', code } }));
  }
  async function handle(request, response) {
    if (request.method === 'GET' && request.url === '/v1/models') {
      response.setHeader('Content-Type', 'application/json');
      response.end(JSON.stringify({ data: [{ id: plan.model, object: 'model', owned_by: 'calibration' }] }));
      return;
    }
    if (request.method !== 'POST' || request.url !== '/v1/chat/completions') return fail(response, 404, 'route');
    if (request.headers.authorization !== 'Bearer ' + key) return fail(response, 403, 'identity');
    let input, accepted;
    try {
      let size = 0;
      const chunks = [];
      for await (const chunk of request) {
        size += chunk.length;
        assert.ok(size <= plan.max_prompt_bytes + 32768, 'Request exceeds limit');
        chunks.push(chunk);
      }
      input = JSON.parse(Buffer.concat(chunks));
      accepted = boundedMessages(input, plan);
    } catch { return fail(response, 400, 'invalid_request'); }
    let row;
    try { row = budget.reserve(accepted.marker, accepted.size); }
    catch { save(); return fail(response, 403, 'allowance'); }
    const abort = new AbortController();
    aborts.add(abort);
    let timedOut = false;
    const timer = setTimeout(() => { timedOut = true; abort.abort(); }, plan.timeout_seconds * 1000);
    response.on('close', () => { if (!response.writableEnded) abort.abort(); });
    row.forwarded_ms = Date.now();
    let state = 'upstream_failed';
    try {
      save(); // Retain the reservation before sending a potentially billable request.
      const body = upstreamBody(input, plan);
      const upstream = await fetch(endpoint, { method: 'POST', redirect: 'error', signal: abort.signal,
        headers: { Authorization: 'Bearer ' + secret, 'Content-Type': 'application/json' },
        body: JSON.stringify(body) });
      row.headers_ms = Date.now();
      row.http_status = upstream.status;
      if (!upstream.ok || !upstream.headers.get('content-type')?.includes('text/event-stream')) {
        await upstream.body?.cancel();
        throw new Error('upstream_response');
      }
      response.writeHead(200, { 'Content-Type': 'text/event-stream', 'Cache-Control': 'no-store' });
      const observer = new StreamObserver(row, budget);
      for await (const chunk of upstream.body) {
        // A partial upstream error frame must not leak before it is validated.
        for (const frame of observer.push(chunk)) {
          if (!response.write(frame)) await once(response, 'drain', { signal: abort.signal });
        }
      }
      observer.finish();
      state = row.usage ? 'complete' : 'usage_missing';
      response.end();
    } catch {
      state = timedOut ? 'timeout' : abort.signal.aborted ? 'client_cancelled'
        : budget.fenced ? 'usage_exceeded_allowance' : 'upstream_failed';
      abort.abort();
      fail(response, 502, state);
    } finally {
      clearTimeout(timer);
      aborts.delete(abort);
      budget.finish(row, state);
      save();
    }
  }
  const server = createServer((request, response) => {
    const task = handle(request, response).catch(() => fail(response, 500, 'proxy_failure'));
    tasks.add(task);
    void task.finally(() => tasks.delete(task));
  });
  server.requestTimeout = 10000;
  server.headersTimeout = 5000;
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  return { url: `http://127.0.0.1:${server.address().port}`, key, snapshot,
    async close() {
      for (const abort of aborts) abort.abort();
      const closed = new Promise(resolve => server.close(resolve));
      server.closeAllConnections();
      await Promise.allSettled([...tasks]);
      await closed;
      save();
    } };
}
