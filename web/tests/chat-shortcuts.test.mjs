import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { act } from 'react';
import { reactFixture } from './react-fixture.mjs';
import { outboxStorage } from './outbox-storage-fixture.mjs';

const w = '11111111-1111-4111-8111-111111111111';
const session = { user: { id: 'ravi', email: 'ravi@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Member' }] };
const directory = { role: 'Member', users: [{ id: 'ravi', name: 'Ravi' }, { id: 'meera', name: 'Meera' }], agents: [] };
const channel = id => ({ id, name: id, kind: 'channel', visibility: 'public', users: ['ravi', 'meera'], agents: [], description: '' });
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const note = (id, author, content, extra = {}) => ({ id, chat: 'orders', author, name: author === 'ravi' ? 'Ravi' : 'Meera', kind: 'user', content, state: 'sent', sequence: 1, revision: 1, at: 100, thread: null, attachments: [], ...extra });
const key = (init, target = document.activeElement ?? document.body) => act(async () => { target.dispatchEvent(new window.KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init })); });

async function conversation(run, messages) {
  const previous = { indexedDB: globalThis.indexedDB, sessionStorage: globalThis.sessionStorage, IntersectionObserver: globalThis.IntersectionObserver };
  globalThis.indexedDB = outboxStorage();
  globalThis.IntersectionObserver = class { observe() {} unobserve() {} disconnect() {} };
  const edits = [];
  try {
    await reactFixture(async fixture => {
      globalThis.sessionStorage = fixture.dom.window.sessionStorage;
      await (await import('../src/workspace-context.ts')).enterWorkspace();
      const { ConversationPane } = await import('../src/react/chat/conversation.tsx');
      await fixture.render(ConversationPane, { chat: channel('orders'), directory, back: () => {} });
      for (let i = 0; i < 6; i++) await fixture.update();
      await run({ ...fixture, edits });
    }, { url: `http://localhost/w/${w}/?view=chat`, fetch: async (path, options = {}) => {
      path = new URL(path, 'http://localhost').pathname;
      if (path.endsWith('/session')) return response(session);
      if (path.endsWith('/edit')) { edits.push([path, JSON.parse(options.body)]); return response({ ok: true }); }
      if (path.endsWith('/messages')) return response({ agents: [], next_before: null, messages });
      return response({ ok: true });
    } });
  } finally { Object.assign(globalThis, previous); }
}

test('a message that mentions you stands out and says so; others do not', async () => {
  await conversation(async () => {
    const mine = document.querySelector('[data-message-id=m1]');
    assert.ok(mine.classList.contains('chat-message-for-me'));
    assert.equal(mine.querySelector('.chat-message-mentions-you').textContent, 'mentions you');
    for (const id of ['m2', 'm3']) assert.equal(document.querySelector(`[data-message-id=${id}]`).classList.contains('chat-message-for-me'), false, id);
  }, [note('m1', 'meera', '[@Ravi](mention:user:ravi) can you approve this?'), note('m2', 'meera', '[@Meera](mention:user:meera) note to self', { sequence: 2 }),
    note('m3', 'ravi', 'I mention [@Ravi](mention:user:ravi) myself', { sequence: 3 })]);
});

test('↑ in the empty composer edits your last message; Escape gives the composer back', async () => {
  await conversation(async ({ change, update, submit, edits }) => {
    const box = () => document.querySelector('.message-composer textarea[aria-label=Message]');
    await change(box(), 'draft');
    box().focus();
    await key({ key: 'ArrowUp' });
    assert.equal(document.querySelector('textarea[aria-label="Edit message"]'), null, 'not while writing a draft');

    await change(box(), '');
    box().focus();
    await key({ key: 'ArrowUp' });
    const editor = document.querySelector('textarea[aria-label="Edit message"]');
    assert.ok(editor);
    assert.equal(editor.closest('[data-message-id]').dataset.messageId, 'r2', 'the newest of your own messages');
    await key({ key: 'Escape' }, editor);
    await update();
    assert.equal(document.querySelector('textarea[aria-label="Edit message"]'), null);
    assert.equal(document.activeElement, box(), 'focus returns to the composer');

    box().focus();
    await key({ key: 'ArrowUp' });
    await change(document.querySelector('textarea[aria-label="Edit message"]'), 'Loading at 4 PM, not 3');
    await submit(document.querySelector('.chat-message-edit'));
    assert.equal(document.querySelector('textarea[aria-label="Edit message"]'), null);
    assert.deepEqual(edits, [[`/api/identity/workspaces/${w}/chats/orders/messages/r2/edit`, { content: 'Loading at 4 PM, not 3' }]]);
  }, [note('r1', 'ravi', 'First'), note('x', 'meera', 'Between', { sequence: 2 }), note('r2', 'ravi', 'Loading at 3', { sequence: 3 })]);
});

test('⌘K opens Search, ⌘/ lists the shortcuts, and Alt+↓ moves through the list, Alt+⇧+↓ to the next unread', async () => {
  const chats = [channel('orders'), channel('dispatch'), channel('alerts')];
  const original = globalThis.EventSource;
  globalThis.EventSource = class { addEventListener() {} close() {} };
  try {
    await reactFixture(async ({ render, update }) => {
      await (await import('../src/workspace-context.ts')).enterWorkspace();
      const { WorkspaceChat } = await import('../src/react/chat/workspace-chat.tsx');
      await render(WorkspaceChat, { href: `http://localhost/w/${w}/?view=chat&conversation=orders` });
      await act(() => new Promise(resolve => setTimeout(resolve, 650)));
      const at = () => new URL(window.location.href).searchParams.get('conversation');
      const order = [...document.querySelectorAll('.chat-directory a.chat-contact')].map(a => new URL(a.href).searchParams.get('conversation'));

      await key({ key: 'k', ctrlKey: true }, document.body);
      assert.ok([...document.querySelectorAll('dialog[open] h2')].some(h => h.textContent === 'Search'));
      await key({ key: 'Escape' }, document.querySelector('dialog[open]'));
      [...document.querySelectorAll('dialog[open]')].forEach(d => d.close());
      await update();

      await key({ key: '/', metaKey: true }, document.body);
      const list = [...document.querySelectorAll('dialog[open]')].find(d => d.textContent.includes('Keyboard shortcuts'));
      assert.match(list.textContent, /Edit your last message.*↑/);
      assert.match(list.textContent, /Next unread conversation/);
      list.close(); await update();

      const from = order.indexOf('orders');
      await key({ key: 'ArrowDown', altKey: true }, document.body);
      assert.equal(at(), order[(from + 1) % order.length]);
      await key({ key: 'ArrowDown', altKey: true, shiftKey: true }, document.body);
      assert.equal(at(), 'alerts', 'the only unread one');
    }, { url: `http://localhost/w/${w}/?view=chat&conversation=orders`, fetch: async path => {
      path = new URL(path, 'http://localhost').pathname;
      if (path.endsWith('/session')) return response(session);
      if (path.endsWith('/message-capabilities')) return response({ live: true });
      if (path.endsWith('/chat-directory')) return response(directory);
      if (path.endsWith('/chats')) return response({ chats, preferences: { available: true, revision: 0, pins: [] } });
      if (path.endsWith('/unread')) return response({ unread: [{ chat: 'alerts', count: 2 }] });
      if (path.endsWith('/messages')) return response({ agents: [], next_before: null, messages: [] });
      return response({});
    } });
  } finally { globalThis.EventSource = original; }
});
