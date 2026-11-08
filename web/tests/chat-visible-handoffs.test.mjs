import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';
import { outboxStorage } from './outbox-storage-fixture.mjs';
import { mergeMessages } from '../src/react/chat/message-state.ts';

// D246: people see agents talking. A handoff asked on the main timeline is posted
// there: the sender's handoff message, the helper's answer, then the sender's reply.
const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=chat`;
const session = { user: { id: 'owner', email: 'owner@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Admin' }] };
const chat = { id: 'ops', name: 'ops', kind: 'channel', description: '', users: ['owner'], agents: ['atlas', 'nova'] };
const directory = { role: 'Admin', users: [{ id: 'owner', name: 'Ananya' }], agents: [{ id: 'atlas', name: 'Atlas' }, { id: 'nova', name: 'Nova' }] };
const task = 'Nova, can you check the walnut stock for Kapoor?';
const handoff = (thread, over = {}) => ({ id: 'h1', parent: 'parent', root: 'parent', child: 'child', request_message: 'request', sender: 'atlas', target: 'nova',
  sender_name: 'Atlas', target_name: 'Nova', principal_name: 'Ananya', chat: 'ops', thread, task, state: 'complete', detail: '',
  task_state: 'complete', job_state: 'finished', phase: null, can_stop: false, ...over });
const at = (sequence, thread = null) => ({ chat: 'ops', sequence, at: sequence, revision: 1, thread, attachments: [] });
/** The person's question, Atlas's reply, its handoff message and Nova's answer, all in `thread`. */
function exchange(thread = null, over = {}) {
  const h = handoff(thread, over);
  return [
    { ...at(1, thread), id: 'ask', author: 'owner', kind: 'user', content: '@Atlas ask Nova about walnut', state: 'sent' },
    { ...at(2, thread), id: 'parent', author: 'atlas', kind: 'agent', content: 'Nova has 42 walnut boards for Kapoor; I reserved them.', state: 'complete', has_task: true, coordination: { incoming: null, outgoing: [h] } },
    { ...at(3, thread), id: 'request', author: 'atlas', kind: 'agent', content: task, state: 'sent', handoff: h },
    { ...at(4, thread), id: 'child', author: 'nova', kind: 'agent', content: '42 walnut boards are in stock for Kapoor.', state: 'complete', has_task: true, coordination: { incoming: h, outgoing: [] } },
  ];
}
const ids = list => list.map(m => m.id);

test('a reply reads after its handoff message and the helper answer beside it', () => {
  const later = { ...at(5), id: 'later', author: 'owner', kind: 'user', content: 'Thanks', state: 'sent' };
  assert.deepEqual(ids(mergeMessages([...exchange(), later])), ['ask', 'request', 'child', 'parent', 'later'], 'main timeline: request, answer, then the final reply');
  const waiting = exchange(null, { state: 'pending', job_state: 'running' }).map(m => m.id === 'child' ? { ...m, state: 'queued', content: '' } : m);
  assert.deepEqual(ids(mergeMessages(waiting)), ['ask', 'request', 'child', 'parent'], 'the same while the helper works');
  assert.deepEqual(ids(mergeMessages(exchange('root'))), ['ask', 'request', 'child', 'parent'], 'inside a thread, in that thread');
  // Before D246 the exchange sat in a thread under the reply; that does not move it.
  const legacy = exchange(null).map(m => ['request', 'child'].includes(m.id) ? { ...m, thread: 'parent' } : m)
    .map(m => m.id === 'parent' ? { ...m, coordination: { incoming: null, outgoing: [handoff('parent')] } } : m);
  assert.deepEqual(ids(mergeMessages(legacy)), ['ask', 'parent', 'request', 'child']);
  // A helper that asks another agent answers after that agent, and the sender after it.
  const nested = handoff(null, { id: 'h2', parent: 'child', child: 'grandchild', request_message: 'request2', sender_name: 'Nova', target_name: 'Orion' });
  const deep = exchange().map(m => m.id === 'child' ? { ...m, coordination: { incoming: m.coordination.incoming, outgoing: [nested] } } : m);
  deep.push({ ...at(5), id: 'request2', author: 'nova', kind: 'agent', content: 'Orion, the delivery date?', state: 'sent', handoff: nested },
    { ...at(6), id: 'grandchild', author: 'orion', kind: 'agent', content: 'Friday.', state: 'complete' },
    { ...at(7), id: 'later', author: 'owner', kind: 'user', content: 'Thanks', state: 'sent' });
  assert.deepEqual(ids(mergeMessages(deep)), ['ask', 'request', 'request2', 'grandchild', 'child', 'parent', 'later']);
});

/** Opens #ops with `messages` as its history. */
async function withChannel(messages, run) {
  const original = { IntersectionObserver: globalThis.IntersectionObserver, indexedDB: globalThis.indexedDB, sessionStorage: globalThis.sessionStorage };
  globalThis.IntersectionObserver = class { observe() {} unobserve() {} disconnect() {} };
  globalThis.indexedDB = outboxStorage();
  try {
    await reactFixture(async fixture => {
      globalThis.sessionStorage = fixture.dom.window.sessionStorage;
      const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
      const { ConversationPane } = await import('../src/react/chat/conversation.tsx');
      const box = await import('../src/react/chat/outbox.ts');
      await fixture.render(ConversationPane, { chat, directory, back: () => {} });
      for (let i = 0; i < 20 && !document.querySelector('[data-message-id]'); i++) await fixture.update();
      try { await run(fixture); } finally { await fixture.update(() => box.stopOutbox()); }
    }, { url, fetch: async path => {
      const at = new URL(path, 'http://localhost').pathname;
      if (at.endsWith('/message-capabilities')) return { ok: true, status: 200, json: async () => ({ live: false }) };
      if (at.endsWith('/chats/ops/messages')) return { ok: true, status: 200, json: async () => ({ agents: ['atlas', 'nova'], next_before: null, messages }) };
      return { ok: true, status: 200, json: async () => session };
    } });
  } finally { Object.assign(globalThis, original); }
}
const order = log => [...log.querySelectorAll(':scope > [data-message-id]')].map(node => node.dataset.messageId);
const article = id => document.querySelector(`[data-message-id="${id}"]`);

test('the channel shows the exchange on its main timeline with one handoff card', async () => {
  await withChannel(exchange(), async () => {
    const log = document.querySelector('[role=log][aria-label=Messages]');
    assert.deepEqual(order(log), ['ask', 'request', 'child', 'parent']);
    const cards = [...log.querySelectorAll('section[aria-label="Handoff to Nova"]')];
    assert.equal(cards.length, 1, 'the card is on the handoff message, not repeated on the reply');
    assert.ok(article('request').contains(cards[0]));
    assert.match(cards[0].textContent, /Atlas → Nova/);
    assert.match(cards[0].textContent, /Agent handoff · Completed/);
    assert.match(cards[0].textContent, /Requested by Ananya/);
    assert.equal(article('request').textContent.split('can you check the walnut stock').length - 1, 1, 'the task shows once');
    const helper = [...cards[0].querySelectorAll('a')].find(a => a.textContent === 'Open helper task');
    assert.match(helper.href, /message=child/);
    assert.doesNotMatch(helper.href, /thread=/, 'the helper answer is on the main timeline');
    assert.match(article('child').textContent, /Helping Atlas · Requested by Ananya\./);
    assert.match(article('parent').textContent, /I reserved them/);
    assert.equal(document.querySelector('.chat-thread-count'), null, 'nothing is hidden in a thread');
  });
});

test('an exchange asked in a thread stays in that thread', async () => {
  const root = { ...at(0), id: 'root', author: 'owner', kind: 'user', content: "Kapoor's order", state: 'sent' };
  await withChannel([root, ...exchange('root')], async ({ click }) => {
    const log = document.querySelector('[role=log][aria-label=Messages]');
    assert.deepEqual(order(log), ['root']);
    assert.equal(log.querySelectorAll('section.channel-handoff').length, 0);
    await click(log.querySelector('.chat-thread-count'));
    const replies = document.querySelector('[role=log][aria-label="Thread replies"]');
    assert.deepEqual(order(replies), ['ask', 'request', 'child', 'parent']);
    assert.equal(replies.querySelectorAll('section[aria-label="Handoff to Nova"]').length, 1);
  });
});
