import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { act, createElement } from 'react';
import { reactFixture } from './react-fixture.mjs';

const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=chat`;
const session = { user: { id: 'owner', email: 'owner@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Admin' }] };
const chat = { id: 'ops', name: 'ops', kind: 'channel', description: '', users: ['owner', 'p'], agents: [] };
const directory = { role: 'Admin', users: [{ id: 'owner', name: 'Ananya' }, { id: 'p', name: 'Priya' }], agents: [] };
const message = (id, sequence, extra = {}) => ({ id, chat: 'ops', author: 'p', kind: 'user', content: `Message ${id}`, state: 'sent', sequence, revision: 1, at: sequence, thread: null, attachments: [], ...extra });
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
// Longer than the 700 ms a message must stay shown before it is read.
const linger = () => act(() => new Promise(resolve => setTimeout(resolve, 800)));

/** Stands in for the browser's visibility reports: `show` says how much of a node is on screen. */
function visibility() {
  const observers = [];
  const original = { IntersectionObserver: globalThis.IntersectionObserver };
  globalThis.IntersectionObserver = class {
    constructor(callback, options) { this.callback = callback; this.options = options; this.targets = []; observers.push(this); }
    observe(node) { this.targets.push(node); } unobserve() {} disconnect() { this.targets = []; }
  };
  return {
    observers,
    show: (node, entry = {}) => act(async () => observers.filter(observer => observer.targets.includes(node))
      .forEach(observer => observer.callback([{ isIntersecting: true, intersectionRatio: 1, intersectionRect: { height: 40 }, ...entry }]))),
    restore: () => Object.assign(globalThis, original),
  };
}
/** A focused, visible window, as when a person is reading. */
function reading(dom, focused = true) {
  Object.defineProperty(dom.window.document, 'hidden', { configurable: true, value: false });
  dom.window.document.hasFocus = () => focused;
}

test('a message is shown enough when most of it, or most of its reading area, is on screen', async () => {
  await reactFixture(async () => {
    const { shownEnough, SEEN_STEPS } = await import('../src/react/chat/receipt.tsx');
    assert.equal(shownEnough(0.6, 60, 584), true, 'most of a short message');
    assert.equal(shownEnough(0.3, 30, 584), false, 'a short message half off screen');
    assert.equal(shownEnough(0.1, 495, 584), true, 'a long message filling the reading area');
    assert.equal(shownEnough(0.02, 99, 584), false, 'only the first lines of a long message');
    // Whatever the height, the browser reports at a share between "fills 60% of the area" and "fills it".
    for (const height of [600, 974, 1500, 4950, 20000, 80000]) {
      const most = Math.min(1, 584 / height);
      assert.ok(SEEN_STEPS.some(step => step > 0 && step <= most && shownEnough(step, step * height, 584)), `a ${height}px message can be read`);
    }
  }, { url });
});

test('a message taller than its reading area is read once it fills the area, in a focused window', async () => {
  const seen = visibility();
  try {
    await reactFixture(async ({ dom, render }) => {
      await (await import('../src/workspace-context.ts')).enterWorkspace();
      const { useSeen } = await import('../src/react/chat/receipt.tsx');
      const read = [];
      const note = id => read.push(id);
      function Probe({ item }) {
        const article = useSeen(item, false, note);
        return createElement('div', { className: 'area', style: { overflowY: 'auto' } }, createElement('article', { ref: article }));
      }
      reading(dom);
      await render(Probe, { item: message('long', 1) });
      const area = document.querySelector('.area'), article = document.querySelector('article');
      Object.defineProperty(area, 'clientHeight', { value: 584 });
      assert.ok(seen.observers.at(-1).options.threshold.length > 2, 'the browser reports at more than "most of the message"');

      await seen.show(article, { intersectionRatio: 0.02, intersectionRect: { height: 99 } });
      await linger();
      assert.deepEqual(read, [], 'its first lines are not the whole message');
      await seen.show(article, { intersectionRatio: 0.1, intersectionRect: { height: 495 } });
      await linger();
      assert.deepEqual(read, ['long'], 'a screenful of it is');

      // The same screenful in a window nobody is looking at reads nothing.
      reading(dom, false);
      await render(Probe, { item: message('unwatched', 2) });
      await seen.show(document.querySelector('article'), { intersectionRatio: 0.1, intersectionRect: { height: 495 } });
      await linger();
      assert.deepEqual(read, ['long']);
    }, { url, fetch: async () => response(session) });
  } finally { seen.restore(); }
});

test('a thread line says how many replies are new until they are read in the thread', async () => {
  const seen = visibility();
  let stream, stopLive, caughtUp = false;
  const original = { EventSource: globalThis.EventSource };
  globalThis.EventSource = class { constructor() { this.listeners = new Map(); stream = this; } addEventListener(name, fn) { this.listeners.set(name, fn); } close() {} };
  const first = { agents: [], next_before: null, messages: [
    message('m1', 1), message('t1', 2, { thread: 'm1', unread: true }), message('t2', 3, { thread: 'm1', unread: true }), message('m2', 4)] };
  try {
    await reactFixture(async ({ dom, render, update, click }) => {
      await (await import('../src/workspace-context.ts')).enterWorkspace();
      const { ConversationPane } = await import('../src/react/chat/conversation.tsx');
      const { startLive } = await import('../src/react/chat/live.ts');
      reading(dom);
      await render(ConversationPane, { chat, directory, back: () => {} });
      await update(() => { stopLive = startLive(); });
      let cursor = 1;
      const live = (events, reset = false) => update(() => stream.listeners.get('changes')({ data: JSON.stringify({ cursor: cursor++, reset, events }) }));
      const arrive = (...messages) => live(messages.map(item => ({ kind: 'message', chat: 'ops', message: item })));
      const line = () => document.querySelector('[role=log] .chat-thread-line');
      const fresh = () => line().querySelector('.chat-thread-new');
      try {
        assert.equal(fresh().textContent, '2 new');
        assert.match(line().getAttribute('aria-label'), /^Open thread: 2 replies · 2 new · last reply /);
        assert.equal(fresh().classList.contains('chat-thread-new-mention'), false);

        // A reply arrives while the conversation is open; later events for it change nothing.
        await arrive(message('t3', 5, { thread: 'm1', for_me: true }));
        assert.equal(fresh().textContent, '3 new');
        assert.ok(fresh().classList.contains('chat-thread-new-mention'), 'one of them is for you');
        await arrive(message('t3', 5, { thread: 'm1', for_me: true, delivered: 1 }), message('own', 6, { thread: 'm1', author: 'owner' }));
        assert.equal(fresh().textContent, '3 new', 'your own reply and a receipt are not new');
        // A stream event for an unread reply never carries history's mark; the mark stays.
        await arrive(message('t1', 2, { thread: 'm1', revision: 2, edited_at: 9 }));
        assert.equal(fresh().textContent, '3 new');

        // Reading the channel does not read the thread; opening it and seeing a reply does.
        await click(line());
        const reply = id => document.querySelector(`dialog [data-message-id="${id}"]`);
        await seen.show(reply('t1'));
        await linger();
        assert.equal(fresh().textContent, '2 new');
        await seen.show(reply('t2')); await seen.show(reply('t3'));
        await linger();
        assert.equal(fresh(), null, 'nothing new is left');
        assert.match(line().getAttribute('aria-label'), /^Open thread: 4 replies · last reply /);
        // The receipt's own event must not bring a read reply back.
        await arrive(message('t3', 5, { thread: 'm1', for_me: true, seen: 1 }));
        assert.equal(fresh(), null);

        // A fresh history read replaces what arrived here with what the server says.
        await arrive(message('t4', 7, { thread: 'm1' }));
        assert.equal(fresh().textContent, '1 new');
        caughtUp = true;
        await live([], true);
        assert.equal(fresh(), null, 'read on another device');
      } finally { await update(() => stopLive?.()); }
    }, { url, fetch: async path => {
      const at = new URL(path, 'http://localhost').pathname;
      if (at.endsWith('/message-capabilities')) return response({ live: true });
      if (at.endsWith('/chats/ops/messages')) return response(caughtUp
        ? { ...first, messages: [...first.messages.map(({ unread, ...item }) => item), message('t3', 5, { thread: 'm1' }), message('own', 6, { thread: 'm1', author: 'owner' }), message('t4', 7, { thread: 'm1' })] }
        : first);
      return response(session);
    } });
  } finally { seen.restore(); Object.assign(globalThis, original); }
});

test('threads with new replies are listed above the messages and open from there', async () => {
  const seen = visibility();
  let stream, stopLive, reads = 0, extra = [];
  const original = { EventSource: globalThis.EventSource };
  globalThis.EventSource = class { constructor() { this.listeners = new Map(); stream = this; } addEventListener(name, fn) { this.listeners.set(name, fn); } close() {} };
  // An old question, carried on the first page for its unread reply, above newer read messages.
  const page = () => ({ agents: [], next_before: 300, messages: [
    message('old', 5, { content: 'Can someone check the **Friday** delivery?' }), message('r1', 9, { thread: 'old', unread: true, for_me: true }),
    message('m1', 301), message('m2', 302), ...extra] });
  try {
    await reactFixture(async ({ dom, render, update, click }) => {
      await (await import('../src/workspace-context.ts')).enterWorkspace();
      const { ConversationPane } = await import('../src/react/chat/conversation.tsx');
      const { startLive } = await import('../src/react/chat/live.ts');
      reading(dom);
      await render(ConversationPane, { chat, directory, back: () => {} });
      await update(() => { stopLive = startLive(); });
      let cursor = 1;
      const arrive = (...messages) => update(() => stream.listeners.get('changes')({ data: JSON.stringify({ cursor: cursor++, events: messages.map(item => ({ kind: 'message', chat: 'ops', message: item })) }) }));
      const bar = () => document.querySelector('[role=region][aria-label="Threads with new replies"]');
      try {
        assert.equal(bar().querySelector('.chat-new-replies-title').textContent, 'New replies in 1 thread');
        const button = bar().querySelector('button');
        assert.equal(button.getAttribute('aria-label'), 'Open thread “Can someone check the Friday delivery?”: 1 new reply, for you');
        assert.ok(button.querySelector('.chat-thread-new-mention'));

        await click(button);
        const dialog = document.querySelector('dialog');
        assert.ok(dialog?.querySelector('[data-message-id="r1"]'), 'the thread opens on its reply');
        await seen.show(dialog.querySelector('[data-message-id="r1"]'));
        await linger();
        assert.equal(bar(), null, 'nothing new is left');

        // A reply arrives in a thread whose question is not loaded: the first page is read again and brings it.
        const before = reads;
        extra = [message('older', 2, { content: 'Stock count for March' }), message('r2', 303, { thread: 'older', unread: true })];
        await arrive(message('r2', 303, { thread: 'older' }));
        assert.equal(reads, before + 1);
        assert.equal(bar().querySelector('button').textContent, 'Stock count for March1 new');
      } finally { await update(() => stopLive?.()); }
    }, { url, fetch: async path => {
      const at = new URL(path, 'http://localhost').pathname;
      if (at.endsWith('/message-capabilities')) return response({ live: true });
      if (at.endsWith('/chats/ops/messages')) { reads++; return response(page()); }
      return response(session);
    } });
  } finally { seen.restore(); Object.assign(globalThis, original); }
});

test('a visible window that is not focused counts as read only while the person uses it', async () => {
  const seen = visibility();
  try {
    await reactFixture(async ({ dom, render }) => {
      await (await import('../src/workspace-context.ts')).enterWorkspace();
      const { useSeen } = await import('../src/react/chat/receipt.tsx');
      const { RECENT_USE_MS } = await import('../src/react/chat/reading-attention.ts');
      const read = [];
      const note = id => read.push(id);
      function Probe({ items }) {
        return createElement('div', null, items.map(item => createElement(Row, { key: item.id, item })));
      }
      function Row({ item }) { return createElement('article', { ref: useSeen(item, false, note), 'data-id': item.id }); }
      const use = (type = 'pointermove') => act(async () => { document.dispatchEvent(new dom.window.Event(type, { bubbles: true })); });
      reading(dom, false);
      await render(Probe, { items: [message('a', 1)] });
      const row = id => document.querySelector(`[data-id="${id}"]`);

      await seen.show(row('a'));
      await linger();
      assert.deepEqual(read, [], 'Cuckoo on another screen, nobody using it');
      await use();
      await linger();
      assert.deepEqual(read, ['a'], 'moving the pointer over it reads what is shown');

      // Scrolling the page itself is not the person.
      await render(Probe, { items: [message('a', 1), message('b', 2)] });
      const realNow = Date.now;
      try {
        Date.now = () => realNow() + RECENT_USE_MS + 1000;
        await act(async () => { document.querySelector('div').dispatchEvent(new dom.window.Event('scroll')); });
        await seen.show(row('b'));
        await linger();
        assert.deepEqual(read, ['a'], 'long after the last use');
        await use('wheel');
        await linger();
        assert.deepEqual(read, ['a', 'b'], 'scrolling with the wheel is using it');
      } finally { Date.now = realNow; }

      // A hidden tab never reads, used or not.
      Object.defineProperty(dom.window.document, 'hidden', { configurable: true, value: true });
      await render(Probe, { items: [message('a', 1), message('b', 2), message('c', 3)] });
      await use(); await seen.show(row('c'));
      await linger();
      assert.deepEqual(read, ['a', 'b']);
    }, { url, fetch: async () => response(session) });
  } finally { seen.restore(); }
});
