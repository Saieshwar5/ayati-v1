import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { act } from 'react';
import { reactFixture } from './react-fixture.mjs';

const w = '11111111-1111-4111-8111-111111111111', url = `http://localhost/w/${w}/?view=chat`;
const session = { user: { id: 'meera', email: 'meera@example.test' }, workspaces: [{ id: w, name: 'Studio', role: 'Member' }] };
const directory = { role: 'Member', users: [{ id: 'meera', name: 'Meera Iyer' }, { id: 'ravi', name: 'Ravi Kumar' }], agents: [] };
const orders = { id: 'orders', name: 'orders', kind: 'channel', visibility: 'public', users: ['meera', 'ravi'], agents: [], description: '' };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const note = (id, author) => ({ id, chat: 'orders', author, name: author, kind: 'user', content: 'Hi', state: 'sent', at: 1 });
// Longer than the 500 ms pause before counts are read again.
const pause = () => act(() => new Promise(resolve => setTimeout(resolve, 650)));

test('unread counts are read again only for changes that can alter them, one read at a time', async () => {
  let stream, stopLive, stopUnread, cursor = 1, count = 2, hold = false;
  const reads = [], held = [];
  const original = globalThis.EventSource;
  globalThis.EventSource = class { constructor() { this.listeners = new Map(); stream = this; } addEventListener(name, fn) { this.listeners.set(name, fn); } close() {} };
  try {
    await reactFixture(async ({ render, update }) => {
      try {
        await (await import('../src/workspace-context.ts')).enterWorkspace();
        const { startLive } = await import('../src/react/chat/live.ts');
        const { startUnreadCounts } = await import('../src/react/chat/unread-counts.ts');
        const { WorkspaceChat } = await import('../src/react/chat/workspace-chat.tsx');
        // The workspace application starts the live stream and the one unread store.
        await update(() => { stopLive = startLive(); stopUnread = startUnreadCounts(); });
        await render(WorkspaceChat, { href: url });
        const emit = events => update(() => stream.listeners.get('changes')({ data: JSON.stringify({ cursor: ++cursor, events }) }));
        const badge = () => document.querySelector('.chat-unread')?.textContent;
        await pause();
        assert.equal(badge(), '2');
        const before = reads.length;

        // Agent pauses, panel/sample/event access and receipts on Meera's own message.
        await emit([{ kind: 'agents' }, { kind: 'panel_access' }, { kind: 'sample_access' }, { kind: 'event_access', chat: 'orders', source: 'scope' },
          { kind: 'message', chat: 'orders', message: note('mine', 'meera') }]);
        await pause();
        assert.equal(reads.length, before, 'nothing that can change her unread arrived');

        count = 3;
        await emit([{ kind: 'message', chat: 'orders', message: note('new', 'ravi') }]);
        await pause();
        assert.equal(reads.length, before + 1, 'a message from someone else is read once');
        assert.equal(badge(), '3');

        // A slow read is never overlapped, so an older answer cannot replace a newer one.
        hold = true;
        await emit([{ kind: 'message', chat: 'orders', message: note('second', 'ravi') }]);
        await pause();
        assert.equal(reads.length, before + 2);
        await emit([{ kind: 'message', chat: 'orders', message: note('third', 'ravi') }]);
        await pause();
        assert.equal(reads.length, before + 2, 'the next read waits for the one in progress');
        hold = false; count = 5;
        await update(() => held.splice(0).forEach(resolve => resolve(response({ unread: [{ chat: 'orders', count: 4 }] }))));
        assert.equal(reads.length, before + 3, 'then reads once more');
        assert.equal(badge(), '5');
      } finally {
        await update(() => { stopUnread?.(); stopLive?.(); stopLive = stopUnread = undefined; });
      }
    }, { url, fetch: async path => {
      if (path.endsWith('/session')) return response(session);
      if (path.endsWith('/message-capabilities')) return response({ live: true });
      if (path.endsWith('/chat-directory')) return response(directory);
      if (path.endsWith('/chats')) return response({ chats: [orders], preferences: { available: true, revision: 0, pins: [] } });
      if (path.endsWith('/unread')) {
        reads.push(path);
        return hold ? new Promise(resolve => held.push(resolve)) : response({ unread: [{ chat: 'orders', count }] });
      }
      return response({});
    } });
  } finally { globalThis.EventSource = original; }
});
