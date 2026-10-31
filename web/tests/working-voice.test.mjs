import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';
import { mergeMessages } from '../src/react/chat/message-state.ts';

const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=chat`;
const session = { user: { id: 'owner', email: 'owner@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Admin' }] };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const directory = { users: [], agents: [{ id: 'ledger', name: 'Ledger' }], role: 'Admin' };
const reply = { id: 'reply', chat: 'chat', author: 'ledger', kind: 'agent', name: 'Ledger', content: '', state: 'queued', at: 1, sequence: 3, revision: 1, can_cancel: true, has_task: true };

async function setup() {
  globalThis.IntersectionObserver = class { observe() {} unobserve() {} disconnect() {} };
  const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
  return (await import('../src/react/chat/message.tsx')).ChatMessage;
}

test('a request no machine has picked up says it is waiting; typing means the agent is at work', async () => {
  await reactFixture(async ({ render }) => {
    const ChatMessage = await setup();
    await render(ChatMessage, { message: reply, directory });
    assert.equal(document.querySelector('[role=status]').textContent, 'Waiting to start');
    assert.equal(document.querySelector('.chat-typing'), null);
    await render(ChatMessage, { message: { ...reply, progress: { state: 'starting', text: '', generation: 1 } }, directory });
    assert.match(document.querySelector('[role=status]').textContent, /Ledger is typing/);
    await render(ChatMessage, { message: { ...reply, progress: { state: 'responding', text: 'Done. 206', generation: 1 } }, directory });
    assert.match(document.body.textContent, /Done\. 206/);
  }, { url, fetch: async path => response(path.endsWith('/session') ? session : {}) });
});

test('"See what it did" on an update opens the request it is about, not the update itself', async () => {
  const reads = [];
  await reactFixture(async ({ render, button, click }) => {
    const ChatMessage = await setup();
    const update = { ...reply, id: 'said', content: "Got it. I'll match the bank lines to invoices.", state: 'complete', has_task: false, update_of: 'reply', can_cancel: false };
    await render(ChatMessage, { message: update, directory });
    assert.match(document.body.textContent, /Got it\. I'll match the bank lines/);
    assert.equal(document.querySelector('[role=status]'), null, 'an update is an ordinary finished message');
    await click(button('More actions'));
    await click(button('See what it did'));
    assert.ok(reads.some(path => path.endsWith('/activity/requests/reply')), JSON.stringify(reads));
    assert.ok(!reads.some(path => path.endsWith('/activity/requests/said')));
  }, { url, fetch: async path => {
    reads.push(path);
    if (path.endsWith('/session')) return response(session);
    if (path.includes('/activity/requests/')) return response({ id: 'reply', chat: 'chat', thread: null, conversation_name: 'Direct', conversation_kind: 'direct', in_conversation: true,
      agent: 'ledger', agent_name: 'Ledger', agent_avatar: null, kind: 'chat', requester: 'owner', requester_name: 'Ravi', summary: 'Reconcile September', state: 'running',
      latest_update: '', created_ms: 1, finished_ms: null, can_cancel: true, approvals: [], message: 'Reconcile September', reply: '', steps: [], fix: null, disagreement: null });
    return response({});
  } });
});

test('a reply reads after what the agent said while working on it, in its own thread only', () => {
  const ask = { id: 'ask', chat: 'c', author: 'owner', kind: 'user', content: 'Reconcile', state: 'sent', at: 1, sequence: 1 };
  const pending = { ...reply, sequence: 2 };
  const gotIt = { id: 'said', chat: 'c', author: 'ledger', kind: 'agent', content: 'Got it.', state: 'complete', at: 2, sequence: 3, update_of: 'reply' };
  const other = { id: 'q', chat: 'c', author: 'kavya', kind: 'user', content: 'Balance?', state: 'sent', at: 3, sequence: 4 };
  const threaded = { ...gotIt, id: 'milestone', content: 'Half done', sequence: 5, thread: 'ask' };
  const ids = list => list.map(m => m.id);
  assert.deepEqual(ids(mergeMessages([ask, pending, gotIt, other])), ['ask', 'said', 'reply', 'q'], 'the reply follows its acknowledgement and stays before later messages');
  assert.deepEqual(ids(mergeMessages([ask, pending, gotIt, other, threaded])), ['ask', 'said', 'reply', 'q', 'milestone'], 'an update in the thread does not move the reply in the timeline');
  const done = { ...pending, state: 'complete', content: 'Done.', revision: 9 };
  assert.deepEqual(ids(mergeMessages([ask, gotIt, other, done])), ['ask', 'said', 'reply', 'q'], 'the same once finished');
});
