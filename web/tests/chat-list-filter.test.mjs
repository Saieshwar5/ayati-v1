import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { act } from 'react';
import { reactFixture } from './react-fixture.mjs';

const w = '11111111-1111-4111-8111-111111111111', url = `http://localhost/w/${w}/?view=chat`;
const session = { user: { id: 'meera', email: 'meera@example.test' }, workspaces: [{ id: w, name: 'Studio', role: 'Member' }] };
const directory = { role: 'Member', users: [{ id: 'meera', name: 'Meera Iyer' }, { id: 'ravi', name: 'Ravi Kumar' }], agents: [] };
const channel = (id) => ({ id, name: id, kind: 'channel', visibility: 'public', users: ['meera', 'ravi'], agents: [], description: '' });
const chats = [channel('orders'), channel('dispatch'), channel('alerts'), channel('lunch')];
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const pause = () => act(() => new Promise(resolve => setTimeout(resolve, 650)));
const listed = () => [...document.querySelectorAll('.chat-directory a.chat-contact')].map(a => a.textContent.match(/^#?\s*([a-z]+)/)?.[1] ?? a.textContent);

async function chatList(run, { unread, levels }) {
  const original = globalThis.EventSource;
  globalThis.EventSource = class { constructor() { this.listeners = new Map(); } addEventListener() {} close() {} };
  try {
    await reactFixture(async fixture => {
      await (await import('../src/workspace-context.ts')).enterWorkspace();
      const { WorkspaceChat } = await import('../src/react/chat/workspace-chat.tsx');
      await fixture.render(WorkspaceChat, { href: url });
      await pause();
      await run(fixture);
    }, { url, fetch: async path => {
      if (path.endsWith('/session')) return response(session);
      if (path.endsWith('/message-capabilities')) return response({ live: true });
      if (path.endsWith('/chat-directory')) return response(directory);
      if (path.endsWith('/chats')) return response({ chats, preferences: { available: true, revision: 0, pins: [] } });
      if (path.endsWith('/chat-notifications')) return response({ levels });
      if (path.endsWith('/unread')) return response({ unread });
      return response({});
    } });
  } finally { globalThis.EventSource = original; }
}

test('Unread only shows just the unread conversations, says how many are read, and is remembered', async () => {
  await chatList(async ({ click }) => {
    const toggle = () => document.querySelector('.chat-unread-only input[role=switch]');
    assert.equal(document.querySelector('.chat-unread-total').textContent, '2');
    assert.equal(document.querySelector('.chat-unread-total').getAttribute('aria-label'), '2 unread conversations');
    assert.equal(listed().length, 4);

    await click(toggle());
    assert.equal(toggle().checked, true);
    assert.deepEqual(listed().sort(), ['dispatch', 'orders']);
    assert.match(document.querySelector('.chat-list-note[role=status]').textContent, /^2 other conversations are read\. Show all$/);
    assert.equal(localStorage.getItem(`relay.chat.unread-only.meera.${w}`), 'true', 'kept per person and workspace');

    await click([...document.querySelectorAll('.chat-list-note button')].find(b => b.textContent === 'Show all'));
    assert.equal(toggle().checked, false);
    assert.equal(listed().length, 4);
  }, { unread: [{ chat: 'orders', count: 3 }, { chat: 'dispatch', count: 1, mentions: 0 }], levels: {} });
});

test('with nothing unread the list says so instead of looking empty', async () => {
  await chatList(async ({ click }) => {
    await click(document.querySelector('.chat-unread-only input'));
    assert.deepEqual(listed(), []);
    assert.match(document.querySelector('.chat-list-note[role=status]').textContent, /^You’re all caught up\. 4 other conversations are read\. Show all$/);
  }, { unread: [], levels: {} });
});

test('a hidden conversation leaves the list until it mentions you, and can be shown on request', async () => {
  await chatList(async ({ click }) => {
    // #alerts is hidden and quiet; #lunch is hidden but mentions Meera, so it is back.
    assert.deepEqual(listed().sort(), ['dispatch', 'lunch', 'orders']);
    const note = [...document.querySelectorAll('.chat-list-note')].find(p => p.textContent.includes('hidden'));
    assert.match(note.textContent, /^1 conversation is hidden until someone mentions you\. Show them$/);
    await click(note.querySelector('button'));
    assert.ok(listed().includes('alerts'));
    assert.equal(note.querySelector('button').getAttribute('aria-expanded'), 'true');
    const alerts = [...document.querySelectorAll('.chat-directory a.chat-contact')].find(a => a.textContent.startsWith('alerts') || a.textContent.includes('alerts'));
    assert.equal(alerts.querySelector('.chat-muted').getAttribute('title'), 'Hidden until someone mentions you');
  }, { unread: [{ chat: 'alerts', count: 9 }, { chat: 'lunch', count: 2, mentions: 1 }], levels: { alerts: 'hide', lunch: 'hide' } });
});

test('hidden conversations never alert, like muted ones', async () => {
  const { shouldAlert } = await import('../src/react/chat/notifications.ts');
  const message = { id: 'm', author: 'ravi', kind: 'user', content: 'Hi', state: 'sent', at: 100, for_me: true };
  const chat = { id: 'alerts', kind: 'channel' };
  assert.equal(shouldAlert(message, chat, 'hide', false, 100, false), false);
  assert.equal(shouldAlert(message, chat, 'mentions', false, 100, false), true);
});
