import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { act } from 'react';
import { reactFixture } from './react-fixture.mjs';

const w = '11111111-1111-4111-8111-111111111111', url = `http://localhost/w/${w}/?view=chat`;
const session = { user: { id: 'owner', email: 'owner@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Admin' }] };
const people = ['owner', 'anita', 'ravi', 'karan', 'meera', 'divya'];
const directory = { role: 'Admin', users: people.map(id => ({ id, name: id[0].toUpperCase() + id.slice(1) })), agents: [] };
const dm = (who, activity) => ({ id: `${who}-dm`, name: who, kind: 'direct', description: '', users: ['owner', who], agents: [], activity });
const channel = (id, activity) => ({ id, name: id, kind: 'channel', description: '', visibility: 'public', users: ['owner'], agents: [], activity });
const chats = [channel('orders', 300), dm('anita', 250), channel('general', 200), dm('ravi', 150), dm('karan', 100), dm('meera', 50)];
const pins = { available: true, revision: 1, pins: [{ kind: 'chat', id: 'meera-dm' }] };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const message = (chat, at, extra = {}) => ({ id: `${chat}-${at}`, chat, author: chat.split('-')[0], kind: 'user', content: 'Hello', state: 'sent', sequence: at, revision: 1, at, thread: null, attachments: [], ...extra });

/** Names in one section of the list, top to bottom. */
const section = label => [...document.querySelectorAll('.chat-directory section')]
  .find(node => node.querySelector('h2')?.textContent === label)
  ?.querySelectorAll('a.chat-contact .chat-name-text') ?? [];
const names = label => [...section(label)].map(node => node.textContent);

async function withList(run, extraChats = () => []) {
  let stream, stopLive, reads = 0;
  const original = globalThis.EventSource;
  globalThis.EventSource = class { constructor() { this.listeners = new Map(); stream = this; } addEventListener(name, fn) { this.listeners.set(name, fn); } close() {} };
  try {
    await reactFixture(async fixture => {
      await (await import('../src/workspace-context.ts')).enterWorkspace();
      const { WorkspaceChat } = await import('../src/react/chat/workspace-chat.tsx');
      const { startLive } = await import('../src/react/chat/live.ts');
      await fixture.render(WorkspaceChat, { href: url });
      await fixture.update(() => { stopLive = startLive(); });
      let cursor = 1;
      const arrive = (...messages) => fixture.update(() => stream.listeners.get('changes')({ data: JSON.stringify({ cursor: cursor++, events: messages.map(m => ({ kind: 'message', chat: m.chat, message: m })) }) }));
      await run({ ...fixture, arrive, reads: () => reads });
      await fixture.update(() => stopLive?.());
    }, { url, fetch: async path => {
      const at = new URL(path, 'http://localhost').pathname;
      if (at.endsWith('/session')) return response(session);
      if (at.endsWith('/chat-directory')) return response(directory);
      if (at.endsWith('/chats')) { reads++; return response({ chats: [...extraChats(), ...chats], preferences: pins }); }
      if (at.endsWith('/unread')) return response({ unread: [] });
      if (at.endsWith('/message-capabilities')) return response({ live: true });
      return response({ levels: {} });
    } });
  } finally { globalThis.EventSource = original; }
}

test('a new message or finished reply moves its conversation to the top; other changes do not', async () => {
  await withList(async ({ arrive }) => {
    assert.deepEqual(names('Direct messages'), ['Anita', 'Ravi', 'Karan'], 'the server order');
    assert.deepEqual(names('Channels'), ['orders', 'general']);
    await arrive(message('karan-dm', 400));
    assert.deepEqual(names('Direct messages'), ['Karan', 'Anita', 'Ravi']);
    await arrive(
      message('ravi-dm', 500, { id: 'note', author: 'agent', kind: 'agent', update_of: 'request' }),
      message('ravi-dm', 120, { reactions: [{ emoji: '👍', count: 1, mine: true }] }),
      message('ravi-dm', 510, { id: 'reply', author: 'agent', kind: 'agent', state: 'queued' }));
    assert.deepEqual(names('Direct messages'), ['Karan', 'Anita', 'Ravi'], 'progress notes, old messages and queued replies');
    await arrive(message('ravi-dm', 130, { id: 'late', author: 'agent', kind: 'agent', state: 'complete', activity_at: 700 }));
    assert.deepEqual(names('Direct messages'), ['Ravi', 'Karan', 'Anita'], 'a reply that finished late');
    await arrive(message('general', 450));
    assert.deepEqual(names('Channels'), ['general', 'orders']);
    await arrive(message('meera-dm', 900));
    assert.deepEqual(names('Pinned'), ['Meera'], 'pinned conversations stay pinned');
    assert.deepEqual(names('Direct messages'), ['Ravi', 'Karan', 'Anita']);
  });
});

test('rows hold still while the mouse moves over the list and catch up when it leaves or rests', async () => {
  await withList(async ({ arrive, update }) => {
    const list = document.querySelector('.chat-directory');
    await update(() => list.dispatchEvent(new window.PointerEvent('pointerover', { bubbles: true, pointerType: 'mouse' })));
    await arrive(message('karan-dm', 400));
    assert.deepEqual(names('Direct messages'), ['Anita', 'Ravi', 'Karan'], 'held');
    await update(() => list.dispatchEvent(new window.PointerEvent('pointerout', { bubbles: true, pointerType: 'mouse', relatedTarget: document.body })));
    assert.deepEqual(names('Direct messages'), ['Karan', 'Anita', 'Ravi']);
    // A resting mouse lets the list catch up after a moment.
    await update(() => list.dispatchEvent(new window.PointerEvent('pointerover', { bubbles: true, pointerType: 'mouse' })));
    await arrive(message('anita-dm', 450));
    assert.deepEqual(names('Direct messages'), ['Karan', 'Anita', 'Ravi'], 'held again');
    await act(() => new Promise(resolve => setTimeout(resolve, 1600)));
    assert.deepEqual(names('Direct messages'), ['Anita', 'Karan', 'Ravi'], 'caught up while the mouse rests');
    await update(() => list.dispatchEvent(new window.PointerEvent('pointerout', { bubbles: true, pointerType: 'mouse', relatedTarget: document.body })));
    // A finger on a touch screen does not hold the list.
    await update(() => list.dispatchEvent(new window.PointerEvent('pointerover', { bubbles: true, pointerType: 'touch' })));
    await arrive(message('ravi-dm', 500));
    assert.deepEqual(names('Direct messages'), ['Ravi', 'Anita', 'Karan']);
  });
});

test('a message for a conversation the list does not have reads the list once', async () => {
  let added = false;
  await withList(async ({ arrive, reads }) => {
    const before = reads();
    added = true;
    await arrive(message('divya-dm', 400));
    assert.equal(reads(), before + 1);
    assert.deepEqual(names('Direct messages'), ['Divya', 'Anita', 'Ravi', 'Karan'], 'a new person appears at once, on top');
    await arrive(message('work-room', 410), message('work-room', 420));
    assert.equal(reads(), before + 2, 'a conversation the list never shows is asked about once');
  }, () => added ? [dm('divya', 400)] : []);
});
