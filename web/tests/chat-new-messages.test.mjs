import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=chat`;
const session = { user: { id: 'owner', email: 'owner@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Admin' }] };
const chat = { id: 'ops', name: 'ops', kind: 'channel', description: '', users: ['owner', 'p'], agents: ['a'] };
const directory = { role: 'Admin', users: [{ id: 'owner', name: 'Ananya' }, { id: 'p', name: 'Priya' }], agents: [{ id: 'a', name: 'Stock assistant' }] };
const message = (id, sequence, extra = {}) => ({ id, chat: 'ops', author: 'p', kind: 'user', content: `Message ${id}`, state: 'sent', sequence, revision: 1, at: sequence, thread: null, attachments: [], ...extra });
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const parse = path => new URL(path, 'http://localhost');

/** Opens the pane with a controllable live stream; `page()` answers each history read. */
async function withPane(page, run) {
  let stream, stopLive;
  const original = { EventSource: globalThis.EventSource, IntersectionObserver: globalThis.IntersectionObserver };
  globalThis.EventSource = class { constructor() { this.listeners = new Map(); stream = this; } addEventListener(name, fn) { this.listeners.set(name, fn); } close() {} };
  // As in chat-message-actions.test.mjs: jsdom has no IntersectionObserver (seen receipts).
  globalThis.IntersectionObserver = class { observe() {} unobserve() {} disconnect() {} };
  try {
    await reactFixture(async fixture => {
      const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
      const { ConversationPane } = await import('../src/react/chat/conversation.tsx');
      const { startLive } = await import('../src/react/chat/live.ts');
      await fixture.render(ConversationPane, { chat, directory, back: () => {} });
      await fixture.update(() => { stopLive = startLive(); });
      let cursor = 1;
      const live = (events, reset = false) => fixture.update(() => stream.listeners.get('changes')({ data: JSON.stringify({ cursor: cursor++, reset, events }) }));
      const arrive = (...messages) => live(messages.map(m => ({ kind: 'message', chat: 'ops', message: m })));
      await run({ ...fixture, list: document.querySelector('[role=log]'), live, arrive });
      await fixture.update(() => stopLive?.());
    }, { url, fetch: async path => {
      const at = parse(path).pathname;
      if (at.endsWith('/message-capabilities')) return response({ live: true });
      if (at.endsWith('/chats/ops/messages')) return response(page());
      return response(session);
    } });
  } finally { Object.assign(globalThis, original); }
}

const line = () => document.querySelector('.chat-unread-line');

test('a conversation shows a line above its first unread message, and the line stays while it is open', async () => {
  const first = { agents: [], next_before: null, messages: [
    message('m1', 1), message('m2', 2, { unread: true }), message('t1', 3, { thread: 'm1', unread: true }), message('m3', 4, { unread: true })] };
  const reread = { ...first, messages: first.messages.map(({ unread, ...m }) => m) };
  let read = false;
  await withPane(() => read ? reread : first, async ({ live, arrive }) => {
    assert.equal(line().getAttribute('aria-label'), '2 unread messages', 'thread replies are not in the main timeline');
    assert.equal(line().nextElementSibling.dataset.messageId, 'm2');
    read = true;
    await live([], true);
    assert.equal(line()?.getAttribute('aria-label'), '2 unread messages', 'reading them does not move or remove the line');
    await arrive(message('mine', 5, { author: 'owner' }));
    assert.equal(line(), null, 'sending a message means you have caught up');
  });
});

test('no unread messages means no line; unread continuing into older pages says so', async () => {
  await withPane(() => ({ agents: [], next_before: null, messages: [message('m1', 1), message('m2', 2)] }), async () => {
    assert.equal(line(), null);
  });
  await withPane(() => ({ agents: [], next_before: 7, messages: [message('m8', 8, { unread: true }), message('m9', 9, { unread: true })] }), async () => {
    assert.equal(line().getAttribute('aria-label'), '2+ unread messages');
  });
});

test('scrolled up, only new finished messages from others are counted, and the button jumps to the latest', async () => {
  await withPane(() => ({ agents: [], next_before: null, messages: [message('m1', 1), message('m2', 2)] }), async ({ dom, list, arrive, button, click, update }) => {
    let top = 1000;
    Object.defineProperty(list, 'scrollHeight', { configurable: true, get: () => 5000 });
    Object.defineProperty(list, 'clientHeight', { configurable: true, get: () => 500 });
    Object.defineProperty(list, 'scrollTop', { configurable: true, get: () => top, set: value => { top = value; } });
    const scroll = () => update(() => list.dispatchEvent(new dom.window.Event('scroll')));
    assert.equal(button('Jump to latest'), undefined, 'hidden at the bottom');
    await scroll();
    assert.ok(button('Jump to latest'));
    await arrive(message('new', 3));
    assert.ok(button('1 new message'));
    await arrive(
      message('reply', 4, { thread: 'm1' }),
      message('note', 5, { author: 'a', kind: 'agent', update_of: 'answer' }),
      message('mine', 6, { author: 'owner' }),
      message('m1', 1, { reactions: [{ emoji: '👍', count: 1, mine: false }] }),
      message('new', 3, { revision: 2 }),
      message('answer', 7, { author: 'a', kind: 'agent', state: 'queued' }));
    assert.ok(button('1 new message'), 'replies, notes, your own, older and repeated messages do not count');
    await arrive(message('answer', 7, { author: 'a', kind: 'agent', state: 'complete', revision: 2 }));
    assert.ok(button('2 new messages'), 'an agent reply counts once it is finished');
    await click(button('2 new messages'));
    assert.equal(top, 5000);
    assert.equal(document.activeElement.dataset.messageId, 'answer', 'focus moves to the newest message');
    assert.equal(document.querySelector('.chat-jump-latest'), null);
    await arrive(message('later', 8));
    assert.equal(document.querySelector('.chat-jump-latest'), null, 'at the bottom nothing is counted');
  });
});
