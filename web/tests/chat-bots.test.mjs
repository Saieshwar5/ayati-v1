import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { act, createElement } from 'react';
import { reactFixture } from './react-fixture.mjs';

const w = '11111111-1111-4111-8111-111111111111', url = `http://localhost/w/${w}/?view=chat`;
const session = { user: { id: 'ravi', email: 'ravi@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Admin' }] };
const directory = { role: 'Admin', users: [{ id: 'ravi', name: 'Ravi' }], agents: [] };
const alerts = { id: 'alerts', name: 'alerts', kind: 'channel', visibility: 'private', users: ['ravi'], agents: [], description: '', can_archive: true };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const settle = async () => { for (let i = 0; i < 5; i++) await act(async () => { await Promise.resolve(); }); };

test('channel managers add a bot, see its address once, and remove it', async () => {
  const writes = [];
  let bots = [];
  await reactFixture(async ({ dom, render, change, submit, click, button }) => {
    const copied = [];
    Object.defineProperty(dom.window.navigator, 'clipboard', { configurable: true, value: { writeText: async text => { copied.push(text); } } });
    Object.defineProperty(globalThis, 'navigator', { configurable: true, value: dom.window.navigator });
    await (await import('../src/workspace-context.ts')).enterWorkspace();
    const { ChannelBots } = await import('../src/react/chat/channel-bots.tsx');
    await render(() => createElement(ChannelBots, { chat: alerts }));
    await settle();
    assert.match(document.body.textContent, /No bots yet\./);
    await change(document.querySelector('input[aria-label="Bot name"]'), '  Uptime Monitor ');
    await submit(document.querySelector('form.chat-bots-add'));
    await settle();
    assert.deepEqual(writes, [{ path: '/chats/alerts/bots', body: { name: 'Uptime Monitor' } }]);
    const shown = document.querySelector('input[aria-label="Webhook address for Uptime Monitor"]');
    assert.equal(shown.value, 'https://cuckoo.example/hooks/chat/b1/s3cret');
    assert.match(document.querySelector('.chat-bots-added').textContent, /shown only once/);
    await click(button('Copy'));
    assert.deepEqual(copied, ['https://cuckoo.example/hooks/chat/b1/s3cret']);
    assert.ok(button('Copied'));
    assert.match(document.querySelector('.chat-bots-list').textContent, /Uptime Monitor.*No posts yet/);
    await click(button('Done'));
    assert.equal(document.querySelector('.chat-bots-added'), null, 'the address is gone for good');

    await click(button('Remove Uptime Monitor'));
    assert.match(document.body.textContent, /Its address stops working at once/);
    await click([...document.querySelectorAll('.chat-bots-confirm button')].find(b => b.textContent === 'Remove'));
    await settle();
    assert.deepEqual(writes.at(-1), { path: '/chats/alerts/bots/b1/remove', body: {} });
    assert.match(document.body.textContent, /No bots yet\./);
  }, { url, fetch: async (path, options = {}) => {
    path = path.replace(`/api/identity/workspaces/${w}`, '');
    if (path.endsWith('/session')) return response(session);
    if (options.method === 'POST') {
      writes.push({ path, body: JSON.parse(options.body) });
      if (path.endsWith('/remove')) { bots = []; return response({ ok: true }); }
      const bot = { id: 'b1', name: 'Uptime Monitor', created: 1, last_post: null, posts_30_days: 0 };
      bots = [bot];
      return response({ bot, url: 'https://cuckoo.example/hooks/chat/b1/s3cret' });
    }
    if (path.endsWith('/bots')) return response({ bots });
    return response({});
  } });
});

test('a bot message is tagged BOT, links to no profile and gets no link preview', async () => {
  const reads = [];
  await reactFixture(async ({ render }) => {
    globalThis.IntersectionObserver = class { constructor(fn) { this.fn = fn; } observe(node) { this.fn([{ isIntersecting: true, target: node }]); } disconnect() {} };
    await (await import('../src/workspace-context.ts')).enterWorkspace();
    const { ChatMessage } = await import('../src/react/chat/message.tsx');
    const message = { id: 'm1', chat: 'alerts', author: 'b1', name: 'Uptime Monitor', kind: 'bot', content: '**Payment page is down** [Open dashboard](https://status.example)', state: 'sent', at: 1 };
    await render(() => createElement(ChatMessage, { message, directory }));
    await settle();
    const article = document.querySelector('[data-message-id=m1]');
    assert.equal(article.getAttribute('aria-label'), 'Message from Uptime Monitor');
    assert.equal(article.querySelector('.ai-tag').textContent, 'BOT');
    assert.equal(article.querySelector('button.chat-message-author'), null);
    assert.equal(article.querySelector('a[href="https://status.example"]').textContent, 'Open dashboard');
    assert.deepEqual(reads, []);
  }, { url, fetch: async path => { if (path.includes('link-preview')) reads.push(path); return response(path.endsWith('/session') ? session : {}); } });
});
