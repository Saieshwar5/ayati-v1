import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { act } from 'react';
import { reactFixture } from './react-fixture.mjs';

const w = '11111111-1111-4111-8111-111111111111';
const at = query => `http://localhost/w/${w}/?view=chat${query}`;
const session = { user: { id: 'meera', email: 'meera@example.test' }, workspaces: [{ id: w, name: 'Studio', role: 'Member' }] };
const directory = { role: 'Member', users: [{ id: 'meera', name: 'Meera Iyer' }, { id: 'arjun', name: 'Arjun Mehta' }], agents: [{ id: 'stock', name: 'Stock assistant' }] };
const orders = { id: 'orders', name: 'orders', kind: 'channel', visibility: 'public', users: ['meera', 'arjun'], agents: ['stock'], description: '' };
const direct = { id: 'dm', name: 'dm', kind: 'direct', users: ['meera', 'arjun'], agents: [], description: '' };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const named = node => { const copy = node.cloneNode(true); copy.querySelectorAll('[aria-hidden=true]').forEach(part => part.remove()); return copy.textContent.replace(/\s+/g, ' ').trim(); };
const pause = () => act(() => new Promise(resolve => setTimeout(resolve, 650)));
const item = (id, reason, extra = {}) => ({ id, chat: 'orders', chat_name: 'orders', chat_kind: 'channel', thread: null, author: 'arjun', kind: 'user', name: 'Arjun Mehta', at: Math.floor(Date.now() / 1000), reason, unread: true, unavailable: false, content: `Message ${id}`, ...extra });

test('the For you helpers place, date, merge and count items', async () => {
  await reactFixture(async () => {
    const { itemPlace, earlierItems, forYouCount } = await import('../src/react/chat/for-you-model.ts');
    assert.equal(itemPlace(item('a', 'mention')), 'in #orders');
    assert.equal(itemPlace(item('a', 'thread', { thread: 'root', chat_kind: 'group', chat_name: 'Operations' })), 'thread in Operations');
    const first = { available: true, new: [item('n', 'mention')], earlier: [item('e1', 'mention', { unread: false })], next_before: 'c' };
    const older = [{ available: true, new: [], earlier: [item('e1', 'mention'), item('e2', 'thread'), item('n', 'mention')], next_before: null }];
    assert.deepEqual(earlierItems(first, older).map(entry => entry.id), ['e1', 'e2'], 'each once, and not one that is new again');
    assert.equal(forYouCount([orders, direct], { orders: 2, dm: 5 }), 2, 'direct messages have their own rows');
  }, { url: at('') });
});

/** Chat with a controllable live stream; `answer` replies to each request. */
async function chatPage(url, answer, run) {
  let stream, stopLive, cursor = 1;
  const original = { EventSource: globalThis.EventSource, IntersectionObserver: globalThis.IntersectionObserver };
  globalThis.EventSource = class { constructor() { this.listeners = new Map(); stream = this; } addEventListener(name, fn) { this.listeners.set(name, fn); } close() {} };
  globalThis.IntersectionObserver = class { observe() {} unobserve() {} disconnect() {} };
  try {
    await reactFixture(async fixture => {
      try {
        await (await import('../src/workspace-context.ts')).enterWorkspace();
        const { startLive } = await import('../src/react/chat/live.ts');
        const { WorkspaceChat } = await import('../src/react/chat/workspace-chat.tsx');
        await fixture.update(() => { stopLive = startLive(); });
        await fixture.render(WorkspaceChat, { href: url });
        const emit = events => fixture.update(() => stream.listeners.get('changes')({ data: JSON.stringify({ cursor: ++cursor, events }) }));
        const until = async condition => { for (let i = 0; i < 60 && !condition(); i++) await fixture.update(); assert.ok(condition(), `Expected UI did not appear: ${document.body.textContent}`); };
        await run({ ...fixture, emit, until });
      } finally { await fixture.update(() => { stopLive?.(); stopLive = undefined; }); }
    }, { url, fetch: async (path, options) => {
      if (path.endsWith('/session')) return response(session);
      if (path.endsWith('/message-capabilities')) return response({ live: true });
      if (path.endsWith('/chat-directory')) return response(directory);
      if (path.endsWith('/chats')) return response({ chats: [orders, direct], preferences: { available: true, revision: 0, pins: [] } });
      if (path.endsWith('/chat-notifications')) return response({ levels: {} });
      if (path.endsWith('/unread')) return response({ unread: [{ chat: 'orders', count: 3, mentions: 2, muted: false }, { chat: 'dm', count: 1, mentions: 1, muted: false }] });
      return answer(path, options) ?? response({});
    } });
  } finally { Object.assign(globalThis, original); }
}

test('For you lists new and earlier items, filters them, loads older ones and follows live changes', async () => {
  const reads = [];
  let first = { available: true, new: [item('m1', 'mention', { content: '[@Meera Iyer](mention:user:meera) please **check** Friday' }), item('a1', 'agent_reply', { author: 'stock', kind: 'agent', name: 'Stock assistant', thread: 'q1', content: 'Stock is fine' })],
    earlier: [item('t1', 'thread', { unread: false, thread: 'q2', content: 'Labels uploaded' })], next_before: 'c1' };
  await chatPage(at('&section=for-you'), path => {
    if (!path.includes('/for-you')) return;
    reads.push(path);
    return response(path.includes('before=c1') ? { available: true, new: [], earlier: [item('t0', 'mention', { unread: false, content: 'An older note' })], next_before: null } : first);
  }, async ({ click, emit, until }) => {
    const row = () => document.querySelector('a.chat-for-you');
    await until(() => document.querySelector('.for-you-item'));
    assert.equal(row().getAttribute('aria-current'), 'page');
    assert.equal(row().querySelector('.chat-unread').textContent, '2', 'the direct message is not counted');
    assert.equal(row().querySelector('.chat-unread').getAttribute('aria-label'), '2 new for you');
    const chips = () => [...document.querySelectorAll('.for-you-filters button')];
    assert.deepEqual(chips().map(named), ['All · 2', '@Mentions · 1', 'Agent replies · 1', 'Threads']);
    const items = () => [...document.querySelectorAll('.for-you-item')];
    assert.equal(items().length, 3);
    const mention = items()[0];
    assert.match(named(mention), /^New: Arjun Mehta @mention in #orders · \d\d:\d\d( [AP]M)? @Meera Iyer please check Friday$/);
    assert.equal(mention.getAttribute('href'), `/w/${w}/?view=chat&conversation=orders&message=m1&from=for-you`);
    assert.match(named(items()[1]), /Stock assistant AI Reply to your request thread in #orders/);
    assert.deepEqual([...document.querySelectorAll('.for-you-section')].map(named), ['New · 2', 'Earlier']);

    await click([...document.querySelectorAll('button')].find(button => button.textContent === 'Older'));
    await until(() => items().length === 4);
    assert.ok(reads.some(path => path.endsWith('/for-you?before=c1')));
    assert.equal([...document.querySelectorAll('button')].some(button => button.textContent === 'Older'), false, 'no more pages');

    await click(chips()[3]);
    assert.equal(chips()[3].getAttribute('aria-pressed'), 'true');
    assert.deepEqual(items().map(entry => entry.getAttribute('href').match(/message=(\w+)/)[1]), ['t1']);
    await click(chips()[0]);

    // Read in its conversation: the next read has it under Earlier.
    const before = reads.length;
    first = { ...first, new: first.new.slice(1), earlier: [{ ...first.new[0], unread: false }, ...first.earlier] };
    await emit([{ kind: 'message', chat: 'orders', message: { id: 'm1', chat: 'orders', author: 'arjun', kind: 'user', content: 'x', state: 'sent', at: 1, seen: 1 } }]);
    await pause();
    assert.equal(reads.length, before + 1, 'read again once');
    assert.deepEqual([...document.querySelectorAll('.for-you-section')].map(named), ['New · 1', 'Earlier']);
    assert.equal(items().length, 4, 'older pages stay, each item once');
  });
});

test('an empty For you explains itself, and losing access clears it', async () => {
  let denied = false;
  await chatPage(at('&section=for-you'), path => {
    if (path.includes('/for-you')) return denied ? response({ error: 'No access' }, 403) : response({ available: true, new: [], earlier: [], next_before: null });
  }, async ({ emit, until }) => {
    await until(() => document.querySelector('.for-you-empty'));
    assert.equal(document.querySelector('.for-you-empty h3').textContent, 'Nothing for you right now');
    assert.equal(document.querySelector('.for-you-empty a').getAttribute('href'), `/w/${w}/?view=activity&tab=needs`);
    denied = true;
    await emit([{ kind: 'access' }]);
    await pause();
    assert.equal(document.querySelector('.for-you-empty'), null);
    assert.equal(document.querySelector('.for-you [role=alert]').textContent, 'No access');
  });
});

test('a conversation opened from For you links back to it', async () => {
  await chatPage(at('&conversation=orders&message=m1&from=for-you'), path => {
    if (path.includes('/chats/orders/messages')) return response({ messages: [{ id: 'm1', chat: 'orders', author: 'arjun', name: 'Arjun Mehta', kind: 'user', content: 'Hello', state: 'sent', at: 1, sequence: 1 }], agents: [], next_before: null });
  }, async ({ until }) => {
    await until(() => document.querySelector('.chat-pane-header'));
    const back = document.querySelector('.chat-pane-header a.chat-return');
    assert.equal(back.textContent, '‹ For you');
    assert.equal(back.getAttribute('href'), `/w/${w}/?view=chat&section=for-you`);
    assert.equal(document.querySelector('a.chat-for-you').getAttribute('aria-current'), null);
  });
});
