// Live messaging at idle and under load, against a fresh `ayati simulate` server.
//
//   node tools/company-simulation/messaging-live-load.mjs http://127.0.0.1:8876 [tabs-per-person] [idle-seconds] [messages]
//
// Signs in the 20 simulated employees, opens tabs-per-person live connections each
// (6 is the per-person limit), then:
//   1. idle: counts batches each connection receives while nothing is sent;
//   2. load: sends messages to a channel of all 20 and times how long each takes
//      to reach every connection (send started → message on the connection).
// Prints JSON. Server-side database load is measured separately (see
// runtime/reliable-messaging.md). Only synthetic simulation data is used.
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';

const [base = 'http://127.0.0.1:8876', tabsArg = '6', idleArg = '60', messagesArg = '50'] = process.argv.slice(2);
const tabs = Number(tabsArg), idleSeconds = Number(idleArg), messages = Number(messagesArg);
const headers = { Origin: base, 'X-Ayati-Request': 'identity', 'Content-Type': 'application/json' };
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const percentile = (values, q) => {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted.length ? Math.round(sorted[Math.min(sorted.length - 1, Math.round((sorted.length - 1) * q))]) : null;
};

async function login(number) {
  const response = await fetch(`${base}/api/identity/simulation/employees/${number}/login`, { method: 'POST', headers, body: '{}' });
  assert.ok(response.ok, `login ${number}: ${response.status}`);
  const cookie = response.headers.getSetCookie().map(c => c.split(';')[0]).join('; ');
  const data = await response.json();
  return { number, cookie, user: data.user, workspace: data.workspace };
}

async function post(person, path, body) {
  const response = await fetch(`${base}${path}`, { method: 'POST', headers: { ...headers, Cookie: person.cookie }, body: JSON.stringify(body) });
  if (!response.ok) throw new Error(`${path}: ${response.status} ${await response.text()}`);
  return response.json();
}

const people = [];
for (let number = 0; number < 20; number++) people.push(await login(number));
const w = people[0].workspace;
const abort = new AbortController();
const connections = [];
const sentAt = new Map();
const arrivals = [];

function open(person) {
  const connection = { person: person.number, batches: 0, idleBatches: 0, seen: new Set(), ready: false };
  connections.push(connection);
  connection.done = (async () => {
    const response = await fetch(`${base}/api/identity/workspaces/${w}/message-events?compact=true`, { headers: { Cookie: person.cookie }, signal: abort.signal });
    assert.ok(response.ok, `stream: ${response.status}`);
    let buffer = '';
    for await (const chunk of response.body) {
      buffer += Buffer.from(chunk).toString();
      let end;
      while ((end = buffer.indexOf('\n\n')) >= 0) {
        const frame = buffer.slice(0, end); buffer = buffer.slice(end + 2);
        const data = frame.split('\n').find(line => line.startsWith('data: '));
        if (!data) continue; // keep-alive comment
        const page = JSON.parse(data.slice(6));
        connection.ready = true;
        connection.batches++;
        if (phase === 'idle') connection.idleBatches++;
        for (const event of page.events ?? []) {
          const id = event.message?.id;
          if (id && sentAt.has(id) && !connection.seen.has(id)) {
            connection.seen.add(id);
            arrivals.push(performance.now() - sentAt.get(id));
          }
        }
      }
    }
  })().catch(error => { if (!abort.signal.aborted) throw error; });
}

let phase = 'connecting';
try {
  for (const person of people) for (let tab = 0; tab < tabs; tab++) open(person);
  for (let waited = 0; !connections.every(c => c.ready); waited += 100) {
    assert.ok(waited < 30000, 'all connections opened');
    await delay(100);
  }
  phase = 'idle';
  const idleStarted = Date.now();
  await delay(idleSeconds * 1000);
  const idleBatches = connections.reduce((n, c) => n + c.idleBatches, 0);
  const idleWall = (Date.now() - idleStarted) / 1000;
  phase = 'load';
  const chat = randomUUID();
  await post(people[0], `/api/identity/workspaces/${w}/chats`, { id: chat, name: `Live load ${Date.now()}`, kind: 'channel',
    description: 'Synthetic live delivery measurement', users: people.map(p => p.user.id), agents: [] });
  const sendTimes = [];
  for (let i = 0; i < messages; i++) {
    const id = randomUUID(), person = people[i % people.length], started = performance.now();
    sentAt.set(id, started);
    await post(person, `/api/identity/workspaces/${w}/chats/${chat}/messages`, { id, content: `Live load message ${i + 1}`, attachments: [] });
    sendTimes.push(performance.now() - started);
    await delay(100);
  }
  for (let waited = 0; connections.some(c => c.seen.size < messages); waited += 100) {
    assert.ok(waited < 60000, 'every connection received every message');
    await delay(100);
  }
  console.log(JSON.stringify({
    connections: connections.length, people: people.length, idle_seconds: Math.round(idleWall),
    idle_batches_received: idleBatches, idle_batches_per_connection_per_minute: +(idleBatches / connections.length / (idleWall / 60)).toFixed(2),
    messages, deliveries: arrivals.length, missing: connections.length * messages - arrivals.length,
    send_ms: { p50: percentile(sendTimes, 0.5), p95: percentile(sendTimes, 0.95), max: percentile(sendTimes, 1) },
    delivery_ms: { p50: percentile(arrivals, 0.5), p95: percentile(arrivals, 0.95), max: percentile(arrivals, 1) },
  }, null, 2));
} finally {
  abort.abort();
  await Promise.allSettled(connections.map(c => c.done));
}
