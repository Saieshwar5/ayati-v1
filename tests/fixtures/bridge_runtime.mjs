#!/usr/bin/env node
// ACP test double for transport correctness. This is not Goose or preparation proof.
// All model traffic is limited to the test's authenticated loopback model bridge.
import { createInterface } from 'node:readline';
import { randomUUID } from 'node:crypto';

if (process.argv.includes('--version')) {
  console.log('1.54.0+ayati.1');
  process.exit(0);
}
const origin = process.env.OPENAI_HOST;
if (!/^http:\/\/127\.0\.0\.1:[0-9]+$/.test(origin ?? '')) throw new Error('Fixture requires a loopback model');
const sessions = new Set();
const active = new Map();
const send = value => process.stdout.write(JSON.stringify({ jsonrpc: '2.0', ...value }) + '\n');
const result = (id, value) => send({ id, result: value });
const update = (sessionId, text) => send({ method: 'session/update', params: { sessionId,
  update: { sessionUpdate: 'agent_message_chunk', content: { type: 'text', text } } } });

async function handle(request) {
  const { id, method, params = {} } = request;
  if (method === 'initialize') return result(id, { protocolVersion: 1, authMethods: [],
    agentCapabilities: { loadSession: true }, agentInfo: { name: 'bridge-runtime-fixture', version: '1' } });
  if (method === 'session/load') {
    sessions.add(params.sessionId);
    return result(id, {});
  }
  if (method === 'session/new') {
    const sessionId = randomUUID();
    sessions.add(sessionId);
    return result(id, { sessionId, _meta: { ayatiPrepared: {
      version: 1, tokenCounter: 'o200k_base', durationMicros: 1,
    } } });
  }
  if (method === 'session/cancel') return active.get(params.sessionId)?.abort();
  if (method !== 'session/prompt') {
    if (id !== undefined) result(id, {});
    return;
  }
  if (!sessions.has(params.sessionId) || active.has(params.sessionId)) throw new Error('Invalid fixture session');
  const stop = new AbortController();
  active.set(params.sessionId, stop);
  try {
    const text = params.prompt.filter(part => part.type === 'text').map(part => part.text).join('\n');
    const response = await fetch(origin + '/v1/chat/completions', { method: 'POST',
      headers: { Authorization: 'Bearer ' + process.env.OPENAI_API_KEY, 'Content-Type': 'application/json' },
      body: JSON.stringify({ model: 'fixture', messages: [{ role: 'user', content: text }], stream: true, tools: [] }),
      signal: stop.signal,
    });
    if (!response.ok) throw new Error('Fixture model refused');
    let buffer = '';
    const decoder = new TextDecoder();
    for await (const bytes of response.body) {
      buffer += decoder.decode(bytes, { stream: true });
      let boundary;
      while ((boundary = buffer.indexOf('\n\n')) >= 0) {
        const event = buffer.slice(0, boundary);
        buffer = buffer.slice(boundary + 2);
        const data = event.split('\n').filter(line => line.startsWith('data:')).map(line => line.slice(5).trim()).join('\n');
        if (!data || data === '[DONE]') continue;
        const text = JSON.parse(data).choices?.[0]?.delta?.content;
        if (text) update(params.sessionId, text);
      }
    }
    result(id, { stopReason: 'end_turn' });
  } catch {
    if (stop.signal.aborted) result(id, { stopReason: 'cancelled' });
    else send({ id, error: { code: -32603, message: 'Scripted runtime model request failed' } });
  } finally { active.delete(params.sessionId); }
}

process.on('SIGTERM', () => { for (const stop of active.values()) stop.abort(); process.exit(0); });
for await (const line of createInterface({ input: process.stdin })) {
  let request;
  try { request = JSON.parse(line); } catch { continue; }
  handle(request).catch(() => {
    if (request.id !== undefined) send({ id: request.id, error: { code: -32603, message: 'Scripted runtime request failed' } });
  });
}
