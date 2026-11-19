import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { act } from 'react';
import { outboxStorage } from './outbox-storage-fixture.mjs';
import { reactFixture } from './react-fixture.mjs';

const w = '11111111-1111-4111-8111-111111111111', url = `http://localhost/w/${w}/?view=home`;
const session = { user: { id: 'meera', email: 'meera@example.test' }, workspaces: [{ id: w, name: 'Studio', role: 'Member' }] };
const directory = { role: 'Member', users: [{ id: 'meera', name: 'Meera Iyer' }, { id: 'ravi', name: 'Ravi Kumar' }], agents: [] };
const channel = id => ({ id, name: id, kind: 'channel', visibility: 'public', users: ['meera', 'ravi'], agents: [], description: '' });
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const note = (id, chat, author) => ({ id, chat, author, name: author, kind: 'user', content: 'Hi', state: 'sent', at: 1 });
// What a screen reader announces: text without the decorative, aria-hidden parts.
const named = node => { const copy = node.cloneNode(true); copy.querySelectorAll('[aria-hidden=true]').forEach(part => part.remove()); return copy.textContent.trim(); };
// Longer than the 500 ms pause before counts are read again.
const pause = () => act(() => new Promise(resolve => setTimeout(resolve, 650)));

test('chatAttention totals what the rail shows: muted conversations add only what is for you', async () => {
  await reactFixture(async () => {
    const { chatAttention } = await import('../src/react/chat/unread-counts.ts');
    assert.deepEqual(chatAttention({ counts: {}, mentions: {}, muted: {} }), { count: 0, forMe: 0 });
    assert.deepEqual(chatAttention({ counts: { a: 2, b: 5 }, mentions: { a: 0, b: 0 }, muted: { a: false, b: true } }), { count: 2, forMe: 0 });
    assert.deepEqual(chatAttention({ counts: { a: 2, b: 5 }, mentions: { a: 1, b: 2 }, muted: { a: false, b: true } }), { count: 4, forMe: 3 });
    // A server from before the flag sends none: nothing is muted.
    assert.deepEqual(chatAttention({ counts: { a: 2, b: 5 }, mentions: {}, muted: {} }), { count: 7, forMe: 0 });
  }, { url });
});

test('the rail shows unread chat messages on every page, from one shared read', async () => {
  let stream, unread = [{ chat: 'orders', count: 2, mentions: 0, muted: false }, { chat: 'quiet', count: 5, mentions: 0, muted: true }], denied = false, cursor = 1;
  const reads = [];
  const original = Object.fromEntries(['indexedDB', 'Event', 'EventSource', 'Option', 'FormData', 'sessionStorage', 'matchMedia', 'requestAnimationFrame', 'cancelAnimationFrame'].map(key => [key, globalThis[key]]));
  await reactFixture(async ({ dom, render, click, update }) => {
    dom.window.setInterval = () => 0; dom.window.clearInterval = () => {};
    Object.assign(globalThis, { indexedDB: outboxStorage(), Event: dom.window.Event, Option: dom.window.Option, FormData: dom.window.FormData, sessionStorage: dom.window.sessionStorage,
      requestAnimationFrame: handler => { handler(); return 0; }, cancelAnimationFrame: () => {}, matchMedia: () => ({ matches: false, addEventListener() {}, removeEventListener() {} }),
      EventSource: class { constructor(address) { this.listeners = new Map(); if (address.includes('message-events')) stream = this; } addEventListener(name, fn) { this.listeners.set(name, fn); } close() {} } });
    try {
      await (await import('../src/workspace-context.ts')).enterWorkspace();
      const { WorkspaceApplication } = await import('../src/react/workspace-application.tsx');
      await render(WorkspaceApplication);
      const until = async condition => { for (let i = 0; i < 80 && !condition(); i++) await update(); assert.ok(condition(), `Expected UI did not appear: ${document.body.textContent}`); };
      const link = () => document.querySelector('.workspace-rail [title=Chat]');
      const count = () => link().querySelector('.workspace-nav-unread');
      const emit = async events => { await update(() => stream.listeners.get('changes')({ data: JSON.stringify({ cursor: ++cursor, events }) })); await pause(); };
      await until(() => document.querySelector('.home') && stream && count());

      // On Home: the muted conversation's five messages stay out of the count.
      assert.equal(count().textContent, '2');
      assert.equal(count().getAttribute('aria-hidden'), 'true');
      assert.equal(named(link()), 'Chat, 2 unread messages');
      assert.equal(count().classList.contains('workspace-nav-unread-mention'), false);

      // A mention counts even where muted, and turns the count red.
      unread = [{ chat: 'orders', count: 2, mentions: 0, muted: false }, { chat: 'quiet', count: 6, mentions: 1, muted: true }];
      await emit([{ kind: 'message', chat: 'quiet', message: note('mention', 'quiet', 'ravi') }]);
      assert.equal(count().textContent, '3');
      assert.equal(named(link()), 'Chat, 3 unread messages, 1 for you');
      assert.ok(count().classList.contains('workspace-nav-unread-mention'));

      // Opening Chat reuses the read: the list and the rail agree without another request.
      const before = reads.length, rail = document.querySelector('.workspace-rail');
      await click(link());
      const row = name => [...document.querySelectorAll('a.chat-contact')].find(a => a.textContent.includes(name));
      await until(() => row('orders')?.querySelector('.chat-unread') && row('quiet')?.querySelector('.chat-unread'));
      assert.equal(row('orders').querySelector('.chat-unread').textContent, '2');
      assert.equal(row('quiet').querySelector('.chat-unread').textContent, '6', 'muting never hides a conversation’s own count');
      assert.equal(document.querySelector('.workspace-rail'), rail);
      assert.equal(link().getAttribute('aria-current'), 'page');
      assert.equal(count().textContent, '3');
      assert.equal(reads.length, before, 'Chat shares the rail’s read');

      // One left reads in the singular; a long backlog stops at 99+.
      unread = [{ chat: 'orders', count: 1, mentions: 0, muted: false }];
      await emit([{ kind: 'message', chat: 'orders', message: note('seen', 'orders', 'ravi') }]);
      assert.equal(named(link()), 'Chat, 1 unread message');
      unread = [{ chat: 'orders', count: 250, mentions: 0, muted: false }];
      await emit([{ kind: 'message', chat: 'orders', message: note('many', 'orders', 'ravi') }]);
      assert.equal(count().textContent, '99+');
      assert.equal(named(link()), 'Chat, 250 unread messages');

      // Back on another page the count stays; reading everything removes it.
      await click(document.querySelector('.workspace-rail [title=Home]'));
      await until(() => document.querySelector('.home'));
      assert.equal(count().textContent, '99+');
      unread = [];
      await emit([{ kind: 'message', chat: 'orders', message: note('read', 'orders', 'ravi') }]);
      assert.equal(count(), null);
      assert.equal(named(link()), 'Chat');

      // Losing access clears a count that was showing.
      unread = [{ chat: 'orders', count: 4, mentions: 0, muted: false }];
      await emit([{ kind: 'message', chat: 'orders', message: note('again', 'orders', 'ravi') }]);
      assert.equal(count().textContent, '4');
      denied = true;
      await emit([{ kind: 'access' }]);
      assert.equal(count(), null);
    } finally {
      await act(async () => { await Promise.resolve(); });
      Object.assign(globalThis, original);
    }
  }, { url, fetch: async path => {
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/message-capabilities')) return response({ live: true });
    if (path.endsWith('/chat-directory')) return response(directory);
    if (path.endsWith('/chats')) return response({ chats: [channel('orders'), channel('quiet')], preferences: { available: true, revision: 0, pins: [] } });
    if (path.endsWith('/chat-notifications')) return response({ levels: { quiet: 'none' } });
    if (path.endsWith('/unread')) { reads.push(path); return denied ? response({ error: 'No access' }, 403) : response({ unread }); }
    if (path.includes('/attention?')) return response({ available: true, items: [], next_before: null });
    if (path.endsWith('/home-panels')) return response({ available: false, panels: [] });
    if (path.endsWith('/work')) return response({ work: [] });
    // Anything else fails as a request would; each feature shows its own error.
    throw new Error(`Unexpected request: ${path}`);
  } });
});
