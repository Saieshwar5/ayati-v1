import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { act } from 'react';
import { reactFixture } from './react-fixture.mjs';
import { outboxStorage } from './outbox-storage-fixture.mjs';

// The rail's unread badge on Chat and the tab title (designs/chat-and-account screen 1, D245).
const w = '11111111-1111-4111-8111-111111111111', other = '22222222-2222-4222-8222-222222222222';
const session = { user: { id: 'meera', email: 'meera@example.test', display_name: 'Meera Iyer' },
  workspaces: [{ id: w, name: 'Lotus Interiors', role: 'Member' }, { id: other, name: 'Kapoor Home Studio', role: 'Member' }] };
const directory = { role: 'Member', agents: [], users: [{ id: 'meera', name: 'Meera Iyer' }, { id: 'kavya', name: 'Kavya Nair' }, { id: 'ravi', name: 'Ravi Kumar' }] };
const conversation = (id, kind, users = ['meera', 'ravi']) => ({ id, name: id, kind, visibility: 'public', users, agents: [], description: '' });
// Screen 2 of the design: two direct messages, three channels (one muted) and a group.
const chats = [conversation('kavya', 'direct', ['meera', 'kavya']), conversation('atlas', 'direct', ['meera']), conversation('orders', 'channel'),
  conversation('site-visits', 'channel'), conversation('leadership', 'channel'), conversation('design', 'group')];
const unread = (chat, count, mentions = 0) => ({ chat, count, mentions });
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const note = (id, chat, author = 'ravi') => ({ id, chat, author, name: author, kind: 'user', content: 'Hi', state: 'sent', at: 1 });
// Longer than the 500 ms pause before counts are read again.
const pause = () => act(() => new Promise(resolve => setTimeout(resolve, 650)));

// What a screen reader announces for the rail's Chat link, and what it shows, as plain values.
const named = node => node.getAttribute('aria-label') ?? node.textContent.trim();
const chatLink = () => [...document.querySelectorAll('nav[aria-label="Workspace"] a[href]')].find(link => named(link).startsWith('Chat'));
const chatName = () => named(chatLink());
const count = () => chatLink().querySelector('.workspace-nav-badge:not(.is-dot)')?.textContent ?? null;
const dot = () => chatLink().querySelector('.workspace-nav-badge.is-dot') !== null;

/** A fake server per workspace. Unread reads can be held to answer later. */
function server() {
  const state = { unread: { [w]: [], [other]: [] }, levels: { [w]: {}, [other]: {} }, chats: { [w]: chats, [other]: [conversation('b-dm', 'direct')] },
    hold: { [w]: false, [other]: false }, held: [], reads: [] };
  state.fetch = async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    const [, workspace, rest] = path.match(/^\/api\/identity\/workspaces\/([^/]+)(\/[^?]*)/) ?? [];
    if (rest === '/unread') {
      state.reads.push({ workspace, signal: options.signal });
      if (state.hold[workspace]) return new Promise(resolve => state.held.push({ workspace, answer: items => resolve(response({ unread: items })) }));
      return response({ unread: state.unread[workspace] });
    }
    if (rest === '/message-capabilities') return response({ live: true });
    if (rest === '/chats') return response({ chats: state.chats[workspace], preferences: { available: true, revision: 0, pins: [] } });
    if (rest === '/chat-notifications') return response({ levels: state.levels[workspace] });
    if (rest === '/chat-directory') return response(directory);
    return response({});
  };
  /** Unread reads that were not cancelled, for one workspace. */
  state.liveReads = workspace => state.reads.filter(read => read.workspace === workspace && !read.signal?.aborted).length;
  return state;
}

/** A workspace page with a scripted live stream; `emit` publishes changes on it. */
async function workspacePage(run, { url = `http://localhost/w/${w}/?view=home`, state = server() } = {}) {
  const keys = ['EventSource', 'indexedDB', 'sessionStorage', 'Event', 'Option', 'FormData', 'matchMedia', 'requestAnimationFrame', 'cancelAnimationFrame'];
  const saved = Object.fromEntries(keys.map(key => [key, globalThis[key]]));
  let stream, cursor = 1;
  try {
    await reactFixture(async context => {
      const { dom, update } = context;
      // The receipt and outbox timer is not under test.
      dom.window.setInterval = () => 0;
      globalThis.EventSource = class { constructor(url) { this.listeners = new Map(); if (url.includes('message-events')) stream = this; } addEventListener(name, fn) { this.listeners.set(name, fn); } close() {} };
      Object.assign(globalThis, { indexedDB: outboxStorage(), sessionStorage: dom.window.sessionStorage, Event: dom.window.Event, Option: dom.window.Option, FormData: dom.window.FormData,
        matchMedia: () => ({ matches: false, addEventListener() {}, removeEventListener() {} }), requestAnimationFrame: fn => { fn(); return 0; }, cancelAnimationFrame: () => {} });
      await (await import('../src/workspace-context.ts')).enterWorkspace();
      const emit = events => update(() => stream.listeners.get('changes')({ data: JSON.stringify({ cursor: ++cursor, events }) }));
      await run({ ...context, emit, state });
    }, { url, fetch: state.fetch });
  } finally { Object.assign(globalThis, saved); }
}

/** The rail alone, with the live stream and the store started as the application does. */
async function rail(state, run) {
  await workspacePage(async context => {
    const { update, render } = context;
    const { startLive } = await import('../src/react/chat/live.ts');
    const store = await import('../src/react/chat/unread-counts.ts');
    const { WorkspaceShell } = await import('../src/react/workspace-shell.tsx');
    let stopLive, stopUnread;
    try {
      await update(() => { stopLive = startLive(); stopUnread = store.startUnreadCounts(); });
      await render(WorkspaceShell, { page: 'home', children: 'Home content' });
      await run({ ...context, store, WorkspaceShell, restart: identity => { stopUnread?.(); stopUnread = store.startUnreadCounts(identity); } });
    } finally { await update(() => { stopUnread?.(); stopLive?.(); }); }
  }, { state });
}

test('the count is direct messages plus what is for you, each once; other channels only light a dot; muted adds nothing', async () => {
  await reactFixture(async () => {
    const { chatBadge, badgeText, badgeLabel } = await import('../src/react/chat/unread-badge.ts');
    const settings = { chats: new Map(chats.map(chat => [chat.id, chat])), levels: { leadership: 'none' }, defaults: { desktop_alerts: true, channel_level: 'mentions', approval_alerts: true } };
    const badge = (items, changed = {}) => chatBadge({ counts: Object.fromEntries(items.map(item => [item.chat, item.count])), mentions: Object.fromEntries(items.map(item => [item.chat, item.mentions])) }, { ...settings, ...changed });
    // Atlas 1 + Kavya 1 + the @mention in #site-visits = 3; #orders and muted #leadership add nothing.
    const screen = [unread('atlas', 1, 1), unread('kavya', 1), unread('orders', 3), unread('site-visits', 1, 1), unread('leadership', 4, 1)];
    assert.deepEqual(badge(screen), { count: 3, dot: false });
    assert.deepEqual(badge([unread('kavya', 2, 2)]), { count: 2, dot: false }, 'a direct message that mentions you counts once');
    assert.deepEqual(badge([unread('design', 3, 1)]), { count: 1, dot: false }, 'groups count what is for you');
    assert.deepEqual(badge([unread('orders', 3), unread('design', 2)]), { count: 0, dot: true }, 'only other channel messages: a dot');
    assert.deepEqual(badge([unread('leadership', 4, 1)]), { count: 0, dot: false }, 'muted adds nothing, not even the dot');
    assert.deepEqual(badge([unread('kavya', 2)], { levels: { kavya: 'none' } }), { count: 0, dot: false }, 'a muted direct message adds nothing');
    assert.deepEqual(badge([unread('kavya', 2), unread('orders', 3, 1)], { defaults: { ...settings.defaults, channel_level: 'none' } }), { count: 2, dot: false }, 'channels muted by default add nothing');
    assert.deepEqual(badge([unread('new-dm', 2, 1)]), { count: 1, dot: false }, 'a conversation not listed yet counts what is for you');
    assert.deepEqual(badge([unread('new-dm', 2)]), { count: 0, dot: true }, 'and lights the dot for the rest');
    assert.deepEqual([badgeText(9), badgeText(99), badgeText(100), badgeText(2500)], ['9', '99', '99+', '99+']);
    assert.deepEqual([badgeLabel('Chat', { count: 3, dot: false }), badgeLabel('Chat', { count: 120, dot: false }), badgeLabel('Chat', { count: 0, dot: true }), badgeLabel('Chat', { count: 0, dot: false })],
      ['Chat, 3 unread', 'Chat, 99+ unread', 'Chat, new messages', 'Chat']);
  }, { url: `http://localhost/w/${w}/`, fetch: async path => response(path.endsWith('/session') ? session : {}) });
});


test('the rail shows the count on every page, follows live messages, a dot for other channels, nothing when muted, and 99+', async () => {
  const state = server();
  state.unread[w] = [unread('atlas', 1, 1), unread('kavya', 1), unread('orders', 3), unread('site-visits', 1, 1), unread('leadership', 4, 1)];
  state.levels[w] = { leadership: 'none' };
  await rail(state, async ({ emit, update, render, store, WorkspaceShell }) => {
    assert.equal(count(), '3');
    assert.equal(chatName(), 'Chat, 3 unread');
    assert.equal(chatLink().querySelector('.workspace-nav-badge').getAttribute('aria-hidden'), 'true', 'the name carries the count; nothing is announced on its own');
    assert.equal(document.querySelector('[aria-live]'), null);

    // Kavya's new message arrives over the live stream.
    state.unread[w] = [unread('atlas', 1, 1), unread('kavya', 2), unread('orders', 3), unread('site-visits', 1, 1), unread('leadership', 4, 1)];
    await emit([{ kind: 'message', chat: 'kavya', message: note('k2', 'kavya', 'kavya') }]);
    await pause();
    assert.equal(count(), '4');

    // Everything for you was read elsewhere; channel messages remain.
    state.unread[w] = [unread('orders', 3), unread('leadership', 4, 1)];
    await emit([{ kind: 'message', chat: 'kavya', message: note('k2', 'kavya', 'kavya') }]);
    await pause();
    assert.deepEqual([count(), dot(), chatName()], [null, true, 'Chat, new messages']);

    state.unread[w] = [unread('leadership', 4, 1)];
    await emit([{ kind: 'message', chat: 'orders', message: note('o1', 'orders') }]);
    await pause();
    assert.deepEqual([count(), dot(), chatName()], [null, false, 'Chat'], 'only a muted conversation is unread');

    state.unread[w] = [unread('kavya', 120), unread('site-visits', 5, 5)];
    await emit([{ kind: 'message', chat: 'kavya', message: note('k3', 'kavya', 'kavya') }]);
    await pause();
    assert.deepEqual([count(), chatName()], ['99+', 'Chat, 99+ unread']);

    // Muting from the bell applies at once.
    await update(() => store.rememberLevel('kavya', 'none'));
    assert.equal(count(), '5');

    // The same badge on the Chat page.
    await render(WorkspaceShell, { page: 'chat', children: 'Chat content' });
    assert.deepEqual([count(), chatLink().getAttribute('aria-current')], ['5', 'page']);
  });
});

test('the rail and the Chat list share one unread read, and the tab title carries the same count', async () => {
  const state = server();
  state.chats[w] = [conversation('kavya', 'direct', ['meera', 'kavya']), conversation('orders', 'channel'), conversation('site-visits', 'channel')];
  state.unread[w] = [unread('kavya', 2), unread('orders', 3), unread('site-visits', 1, 1)];
  await workspacePage(async ({ render, update, emit }) => {
    const { WorkspaceApplication } = await import('../src/react/workspace-application.tsx');
    const row = name => [...document.querySelectorAll('a.chat-contact')].find(link => link.textContent.includes(name));
    const listCount = name => row(name)?.querySelector('.chat-unread')?.textContent ?? null;
    document.title = 'Cuckoo';
    await render(WorkspaceApplication);
    for (let i = 0; i < 40 && !(row('orders') && count()); i++) await update();
    assert.equal(state.liveReads(w), 1, 'one /unread read for the rail and the list');
    assert.deepEqual([count(), listCount('Kavya'), listCount('orders'), listCount('site-visits')], ['3', '2', '3', '1']);
    assert.equal(document.title, '(3) Chat · Cuckoo');

    state.unread[w] = [unread('kavya', 3), unread('orders', 3), unread('site-visits', 1, 1)];
    await emit([{ kind: 'message', chat: 'kavya', message: note('k1', 'kavya', 'kavya') }]);
    await pause();
    assert.equal(state.liveReads(w), 2, 'a live message is read once for both');
    assert.deepEqual([count(), listCount('Kavya'), document.title], ['4', '3', '(4) Chat · Cuckoo']);

    state.unread[w] = [unread('orders', 3)];
    await emit([{ kind: 'message', chat: 'kavya', message: note('k1', 'kavya', 'kavya') }]);
    await pause();
    assert.deepEqual([count(), dot(), document.title], [null, true, 'Chat · Cuckoo'], 'no count: the normal title');

    await render(() => null);
    assert.equal(document.title, 'Cuckoo', 'leaving restores the title');
  }, { url: `http://localhost/w/${w}/?view=chat`, state });
});

test('switching workspace clears the badge at once and ignores the old workspace\'s late answer', async () => {
  const state = server();
  state.unread[w] = [unread('kavya', 3)];
  state.unread[other] = [unread('b-dm', 1)];
  await rail(state, async ({ emit, update, restart }) => {
    assert.equal(count(), '3');
    // A read for the first workspace is still on its way when the person switches.
    state.hold[w] = true;
    await emit([{ kind: 'message', chat: 'kavya', message: note('k1', 'kavya', 'kavya') }]);
    await pause();
    assert.equal(state.held.length, 1);
    state.hold[other] = true;
    await update(() => restart({ workspace: other, user: 'meera' }));
    assert.deepEqual([count(), dot(), chatName()], [null, false, 'Chat'], 'nothing from the previous workspace remains');

    await update(() => state.held.find(read => read.workspace === w).answer([unread('kavya', 9)]));
    assert.deepEqual([count(), dot()], [null, false], 'the late answer for the old workspace is ignored');
    await update(() => state.held.find(read => read.workspace === other).answer(state.unread[other]));
    assert.deepEqual([count(), chatName()], ['1', 'Chat, 1 unread']);
  });
});
