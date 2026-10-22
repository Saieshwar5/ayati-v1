// Owner/User workspace boundary rehearsal. Run after the business/load/recovery scripts:
// this intentionally removes employee 19 from the seeded workspace in its final scenario.
import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { performance } from 'node:perf_hooks';
import { clients, randomUUID, until, distribution } from './client.mjs';

const root = resolve(process.argv[2] ?? '.ayati-local/company-simulation/runs');
const { directory } = JSON.parse(await readFile(join(root, 'latest.json')));
const c = JSON.parse(await readFile(join(directory, 'manifest.json')));
assert.equal(new URL(c.base).hostname, '127.0.0.1');
const verifyRemoval = process.argv.includes('--verify-removal');
const previous = verifyRemoval ? JSON.parse(await readFile(join(directory, 'workspace-report.json'))) : undefined;
const samples = [], checks = previous ? previous.checks.filter(check => check.passed && !check.name.startsWith('Removing a User')) : [];
const { request, sessions } = clients(c.base, samples);
const roster = await request(0, '/api/identity/simulation');
assert.equal(c.w, `/api/identity/workspaces/${roster.workspace}`);
for (const e of c.employees) await request(e.number, `/api/identity/simulation/employees/${e.number}/login`, {});
const member = number => `${c.w}/members/${c.employees[number].user.id}`;
const send = (actor, chat, input, expected = 200) => request(actor, `${c.w}/chats/${chat}/messages`, input, { expected });
const check = async (name, fn) => {
  if (verifyRemoval && !name.startsWith('Removing a User')) return;
  const start = performance.now();
  try { await fn(); checks.push({ name, passed: true, ms: performance.now() - start }); console.log(`PASS ${name}`); }
  catch (error) { checks.push({ name, passed: false, error: String(error) }); console.log(`FAIL ${name}: ${error}`); }
  await writeFile(join(directory, 'workspace-checks.json'), JSON.stringify(checks, null, 2));
};
let solo = previous?.solo_workspace;
await check('Solo creation: concurrent exact retries create one workspace; altered and stolen IDs fail', async () => {
  const input = { name: 'Independent solo workspace', request_id: randomUUID() };
  const results = await Promise.all(Array.from({ length: 8 }, () => request(19, '/api/identity/workspaces', input)));
  assert.equal(new Set(results.map(r => r.id)).size, 1);
  solo = `/api/identity/workspaces/${results[0].id}`;
  assert.equal((await request(19, `${solo}/people`)).members.length, 1);
  await request(19, '/api/identity/workspaces', { ...input, name: 'Different payload' }, { expected: 400 });
  await request(0, '/api/identity/workspaces', input, { expected: 400 });
  assert.equal((await request(19, '/api/identity/session')).workspaces.length, 2);
});
await check('Invalid names, IDs and unknown fields do not create workspaces', async () => {
  for (const name of ['', '  ', 'x'.repeat(81), 'bad\nname']) {
    await request(1, '/api/identity/workspaces', { name, request_id: randomUUID() }, { expected: 400 });
  }
  await request(1, '/api/identity/workspaces', { name: 'Valid', request_id: 'invalid' }, { expected: 400 });
  await request(1, '/api/identity/workspaces', { name: 'Valid', request_id: randomUUID(), role: 'Owner' }, { expected: 422 });
});
await check('User administration denial; people remain discoverable through Chat', async () => {
  for (const path of ['people', 'settings', 'activity']) await request(1, `${c.w}/${path}`, undefined, { expected: 400 });
  await request(1, `${c.w}/invitations`, { email: 'forbidden@example.test', role: 'User' }, { expected: 400 });
  await request(1, member(1), { role: 'Owner' }, { expected: 400 });
  await request(1, `${c.w}/settings`, { name: 'Forged', previous_name: 'anything' }, { expected: 400 });
  await request(1, `${c.w}/teams`, { id: randomUUID(), name: 'Forged', users: [] }, { expected: 400 });
  const directory = await request(1, `${c.w}/chat-directory`);
  assert.equal(directory.users.length, 20);
  assert.equal(directory.invitations, undefined);
});
await check('Same person can own a solo workspace and remain User elsewhere; no cross-workspace reads', async () => {
  assert.equal((await request(19, `${solo}/people`)).role, 'Owner');
  await request(19, `${c.w}/people`, undefined, { expected: 400 });
  await request(0, `${solo}/people`, undefined, { expected: 400 });
  await request(19, `${solo}/chats/${c.channels.Support}/messages`, undefined, { expected: 400 });
  await request(19, `${solo}/settings`, { name: 'My solo workspace', previous_name: 'Independent solo workspace' });
  await request(19, `${solo}/members/${c.employees[19].user.id}`, { role: null }, { expected: 400 });
});
await check('Equal Owners can administer, race stale saves, and demote themselves without losing the final Owner', async () => {
  for (const number of [17, 18]) await request(0, member(number), { role: 'Owner' });
  assert.equal((await request(17, `${c.w}/people`)).role, 'Owner');
  const previous = (await request(0, `${c.w}/settings`)).name;
  const names = ['Concurrent edit A', 'Concurrent edit B'];
  const results = await Promise.all(names.map(async (name, i) => {
    const response = await fetch(c.base + `${c.w}/settings`, {
      method: 'POST', headers: { Origin: c.base, 'X-Ayati-Request': 'identity', Cookie: sessions.get(17 + i), 'Content-Type': 'application/json' },
      body: JSON.stringify({ name, previous_name: previous }), signal: AbortSignal.timeout(30000),
    });
    await response.arrayBuffer(); return response.status;
  }));
  assert.deepEqual(results.sort(), [200, 400]);
  const current = (await request(0, `${c.w}/settings`)).name;
  await request(18, `${c.w}/settings`, { name: previous, previous_name: current });
  await Promise.all([17, 18].map(number => request(number, member(number), { role: 'User' })));
  for (const number of [17, 18]) await request(number, `${c.w}/settings`, undefined, { expected: 400 });
  await request(0, member(0), { role: 'User' }, { expected: 400 });
  await request(0, member(0), { role: null }, { expected: 400 });
  await request(0, member(1), { role: 'Admin' }, { expected: 400 });
});
await check('Uncertain email is visible and cannot silently resend', async () => {
  const email = `audit-${randomUUID()}@example.test`;
  const input = { email, role: 'User' };
  await request(0, `${c.w}/invitations`, input, { expected: 400 });
  const invitation = (await request(0, `${c.w}/people`)).invitations.find(i => i.email === email);
  assert.equal(invitation.status, 'delivery_unknown');
  await request(0, `${c.w}/invitations`, input, { expected: 400 });
  assert.equal((await request(0, `${c.w}/people`)).invitations.filter(i => i.email === email).length, 1);
  await request(1, `${c.w}/invitations/${invitation.id}/cancel`, {}, { expected: 400 });
  await request(19, `${solo}/invitations/${invitation.id}/cancel`, {}, { expected: 400 });
  await request(0, `${c.w}/invitations/${invitation.id}/cancel`, {});
  await request(0, `${c.w}/invitations`, input, { expected: 400 });
  const retry = (await request(0, `${c.w}/people`)).invitations.find(i => i.email === email);
  assert.notEqual(retry.id, invitation.id);
  await request(0, `${c.w}/invitations/${retry.id}/cancel`, {});
  await request(0, `${c.w}/invitations`, { email, role: 'Owner' }, { expected: 400 });
});
await check('CSRF, missing sessions and the ordinary-server simulation fence', async () => {
  for (const headers of [
    { Origin: 'https://untrusted.example', 'X-Ayati-Request': 'identity', Cookie: sessions.get(0) },
    { Origin: c.base, Cookie: sessions.get(0) },
  ]) {
    const result = await fetch(c.base + member(1), { method: 'POST', headers: { ...headers, 'Content-Type': 'application/json' }, body: JSON.stringify({ role: 'Owner' }) });
    assert.equal(result.status, 403);
  }
  assert.equal((await fetch(c.base + `${c.w}/people`)).status, 401);
  // Read-only check: never send a simulated-login request to the ordinary server.
  const ordinary = await fetch('http://127.0.0.1:8765/api/identity/simulation');
  assert.equal(ordinary.status, 404);
});
await check('Revoking an agent grant cancels already-queued work without losing the request', async () => {
  await request(0, '/api/identity/simulation/dispatch', { enabled: false });
  const root = randomUUID(), chat = c.channels.Management;
  await send(1, chat, { id: root, content: 'Access-revocation test thread.' });
  await send(1, chat, { id: randomUUID(), thread: root, content: 'Acknowledge this request.', agent: c.agents[1].id });
  await request(0, `${c.w}/chat-grants`, { user: c.employees[1].user.id, agent: c.agents[1].id, allowed: false });
  await request(0, '/api/identity/simulation/dispatch', { enabled: true });
  const history = await until(() => request(0, `${c.w}/chats/${chat}/messages`), h => h.messages.some(m => m.thread === root && m.kind === 'agent' && m.state !== 'queued'), 'revoked request cancellation');
  assert.equal(history.messages.find(m => m.thread === root && m.kind === 'agent').state, 'cancelled');
  await request(0, `${c.w}/chat-grants`, { user: c.employees[1].user.id, agent: c.agents[1].id, allowed: true });
});
await check('Queue admits 64 requests, rejects overflow, keeps chat usable, and accepts exact retries once', async () => {
  await until(() => request(0, `${c.w}/metrics`), m => m.pending === 0, 'empty queue');
  await request(0, '/api/identity/simulation/dispatch', { enabled: false });
  const root = randomUUID(), chat = c.channels.Management;
  await send(0, chat, { id: root, content: 'Queue-bound test thread.' });
  const payloads = Array.from({ length: 80 }, () => ({ id: randomUUID(), thread: root, content: 'Bounded queue request.', agent: c.agents[0].id }));
  // All offered concurrently; each status is checked without treating overflow as a test failure.
  const responses = await Promise.all(payloads.map(async payload => {
    const response = await fetch(c.base + `${c.w}/chats/${chat}/messages`, { method: 'POST', headers: { Origin: c.base, 'X-Ayati-Request': 'identity', Cookie: sessions.get(0), 'Content-Type': 'application/json' }, body: JSON.stringify(payload), signal: AbortSignal.timeout(30000) });
    await response.arrayBuffer(); return response.status;
  }));
  assert.equal(responses.filter(s => s === 200).length, 64);
  assert.equal(responses.filter(s => s === 429).length, 16);
  assert.equal((await request(0, `${c.w}/metrics`)).pending, 64);
  const accepted = payloads.filter((_, i) => responses[i] === 200);
  await send(0, chat, accepted[0]);
  await send(0, chat, { ...accepted[0], content: 'Altered retry' }, 400);
  await send(0, chat, { id: randomUUID(), content: 'Ordinary chat still works while the agent queue is full.' });
  await request(0, '/api/identity/simulation/dispatch', { enabled: true });
  await until(() => request(0, `${c.w}/metrics`), m => m.pending === 0, 'bounded burst drain');
  const messages = []; let before;
  do { const page = await request(0, `${c.w}/chats/${chat}/messages${before ? `?before=${before}` : ''}`); messages.push(...page.messages); before = page.next_before; } while (before);
  for (const payload of accepted) assert.equal(messages.filter(m => m.id === payload.id).length, 1);
  const replies = messages.filter(m => m.thread === root && m.kind === 'agent');
  assert.equal(replies.length, 64); assert.ok(replies.every(m => m.state === 'complete'));
});
await check('Removing a User fences queued work/history immediately and preserves their other workspace', async () => {
  const chat = c.channels.Support;
  let root;
  if (verifyRemoval) {
    const history = await request(0, `${c.w}/chats/${chat}/messages`);
    root = history.messages.find(m => m.content === 'Retain shared history after removing my membership.').id;
  } else {
    await request(0, '/api/identity/simulation/dispatch', { enabled: false });
    root = randomUUID();
    await send(19, chat, { id: root, content: 'Retain shared history after removing my membership.' });
    await send(19, chat, { id: randomUUID(), thread: root, content: 'Queued before removal.', agent: c.agents[19].id });
    await request(0, member(19), { role: null });
  }
  await request(19, `${c.w}/chat-directory`, undefined, { expected: 403 });
  for (const path of ['settings', `chats/${chat}/messages`]) await request(19, `${c.w}/${path}`, undefined, { expected: 400 });
  await send(19, chat, { id: randomUUID(), content: 'Attempt after revocation' }, 400);
  assert.equal((await request(19, `${solo}/people`)).role, 'Owner');
  assert.equal((await request(19, '/api/identity/session')).workspaces.length, 1);
  await request(0, '/api/identity/simulation/dispatch', { enabled: true });
  const history = await until(() => request(0, `${c.w}/chats/${chat}/messages`), h => h.messages.some(m => m.thread === root && m.kind === 'agent' && m.state !== 'queued'), 'removed requester cancellation');
  assert.equal(history.messages.find(m => m.thread === root && m.kind === 'agent').state, 'cancelled');
  assert.equal(history.messages.filter(m => m.id === root).length, 1);
});
// Always leave dispatch enabled, even when an assertion failed earlier.
await request(0, '/api/identity/simulation/dispatch', { enabled: true });
const report = { checks, passed: checks.filter(c => c.passed).length, failed: checks.filter(c => !c.passed).length, resumed_removal_verification: verifyRemoval, sampled_requests: samples.length, latency: distribution(samples.map(s => s.ms)), solo_workspace: solo };
await writeFile(join(directory, 'workspace-report.json'), JSON.stringify(report, null, 2));
console.log(JSON.stringify(report, null, 2));
process.exitCode = report.failed ? 1 : 0;
