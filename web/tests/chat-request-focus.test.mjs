import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { reactFixture } from './react-fixture.mjs';

const w = '11111111-1111-4111-8111-111111111111';
const session = { user: { id: 'owner', email: 'owner@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Admin' }] };
const chat = { id: 'source', name: 'Procurement', kind: 'channel', users: ['owner'], agents: [] };
const directory = { role: 'Admin', users: [{ id: 'owner', name: 'Ananya' }], agents: [] };
const message = (id, content, thread) => ({ id, chat: 'source', author: 'owner', kind: 'user', content, state: 'sent', sequence: 1, revision: 1, at: 1, thread, attachments: [] });
const latest = { messages: [message('latest', 'Recent conversation')], agents: [], next_before: 400 };
const focused = { messages: [message('root', 'Original thread'), message('reply', 'Saved original response', 'root')], agents: [], next_before: null };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const props = { chat, directory, back: () => {}, focus: 'reply' };

test('a saved request opens its older thread and clears focused history after a denied linked read', async () => {
  const writes = [];
  await reactFixture(async ({ render }) => {
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { ConversationPane } = await import('../src/react/chat/conversation.tsx');
    await render(ConversationPane, props);
    assert.match(document.querySelector('dialog').textContent, /Original thread/);
    assert.equal(document.querySelector('.chat-message-focused').dataset.messageId, 'reply');
    await render(ConversationPane, { ...props, focus: 'denied' });
    assert.equal(document.body.textContent.includes('Saved original response'), false);
    assert.equal(document.querySelector('dialog'), null);
    assert.match(document.querySelector('[role=alert]').textContent, /linked request is unavailable/);
    assert.match(document.body.textContent, /Recent conversation/);
    assert.deepEqual(writes, []);
  }, { url: `http://localhost/w/${w}/?view=chat`, fetch: async (path, options = {}) => {
    path = new URL(path, 'http://localhost').pathname;
    if (options.method === 'POST') writes.push(path);
    return response(path.endsWith('/session') ? session : path.endsWith('/messages/reply') ? focused : path.endsWith('/messages/denied') ? { error: 'Source unavailable' } : latest, path.endsWith('/messages/denied') ? 403 : 200);
  } });
});

test('unmount aborts a linked history read and its late response cannot reopen private work', async () => {
  const pending = [];
  await reactFixture(async ({ render, update }) => {
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { ConversationPane } = await import('../src/react/chat/conversation.tsx');
    await render(ConversationPane, props);
    assert.ok(pending.length);
    await render(() => createElement('p', null, 'Another workspace view'));
    assert.ok(pending.every(read => read.signal.aborted));
    await update(() => pending.forEach(read => read.finish(response(focused))));
    assert.equal(document.body.textContent.includes('Saved original response'), false);
    assert.equal(document.querySelector('dialog'), null);
  }, { url: `http://localhost/w/${w}/?view=chat`, fetch: async (path, options = {}) => {
    path = new URL(path, 'http://localhost').pathname;
    if (path.endsWith('/messages/reply')) return new Promise(finish => pending.push({ finish, signal: options.signal }));
    return response(path.endsWith('/session') ? session : latest);
  } });
});
