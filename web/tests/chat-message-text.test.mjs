import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { act, createElement, Fragment } from 'react';
import { reactFixture } from './react-fixture.mjs';

const w = '11111111-1111-4111-8111-111111111111', url = `http://localhost/w/${w}/?view=chat`;
const session = { user: { id: 'meera', email: 'meera@example.test' }, workspaces: [{ id: w, name: 'Studio', role: 'Member' }] };
const directory = { role: 'Member', users: [{ id: 'meera', name: 'Meera Iyer' }, { id: 'ravi', name: 'Ravi Kumar' }], agents: [] };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const note = (id, content, extra = {}) => ({ id, chat: 'orders', author: 'ravi', name: 'Ravi Kumar', kind: 'user', content, state: 'sent', at: 1, revision: 1, ...extra });
const article = id => document.querySelector(`[data-message-id="${id}"]`);

async function show(render, list, seen = []) {
  // Messages count as on screen as soon as they are watched.
  globalThis.IntersectionObserver = class { constructor(fn) { this.fn = fn; } observe(node) { seen.push(node); this.fn([{ isIntersecting: true, target: node }]); } unobserve() {} disconnect() {} };
  await (await import('../src/workspace-context.ts')).enterWorkspace();
  const { ChatMessage } = await import('../src/react/chat/message.tsx');
  await render(() => createElement(Fragment, null, list.map(message => createElement(ChatMessage, { key: message.id, message, directory }))));
  for (let i = 0; i < 4; i++) await act(async () => { await Promise.resolve(); });
}

test('a code block has a Copy button that copies just its code', async () => {
  await reactFixture(async ({ dom, render, click }) => {
    const copied = [];
    Object.defineProperty(dom.window.navigator, 'clipboard', { configurable: true, value: { writeText: async text => { copied.push(text); } } });
    await show(render, [note('code', 'Run this:\n\n```sh\ncargo test --lib\n```\nthen tell me')]);
    const button = article('code').querySelector('pre.message-code button.message-code-copy');
    assert.equal(button.getAttribute('aria-label'), 'Copy code');
    await click(button);
    await act(async () => { await Promise.resolve(); });
    assert.deepEqual(copied, ['cargo test --lib']);
    assert.equal(button.textContent, 'Copied');
    // Typed HTML is still text: nobody can add a button of their own.
    const { messageHtml } = await import('../src/markdown.ts');
    assert.doesNotMatch(messageHtml('<button class="message-code-copy">x</button>'), /<button/);
  }, { url, fetch: async path => response(path.endsWith('/session') ? session : {}) });
});

test('a long message folds behind Show more; a short one does not', async () => {
  await reactFixture(async ({ render, click }) => {
    const long = Array.from({ length: 20 }, (_, i) => `Line ${i + 1} of the dispatch plan`).join('\n');
    await show(render, [note('long', long), note('short', 'Truck at 4')]);
    const toggle = article('long').querySelector('button.message-fold');
    assert.equal(toggle.textContent, 'Show more');
    assert.ok(article('long').querySelector('.message-markdown').classList.contains('message-folded'));
    await click(toggle);
    assert.equal(toggle.getAttribute('aria-expanded'), 'true');
    assert.equal(toggle.textContent, 'Show less');
    assert.equal(article('long').querySelector('.message-folded'), null);
    assert.equal(article('short').querySelector('button.message-fold'), null);
  }, { url, fetch: async path => response(path.endsWith('/session') ? session : {}) });
});

test('the first link of a message on screen gets one preview read; no link or an agent reply reads none', async () => {
  const reads = [];
  await reactFixture(async ({ render }) => {
    await show(render, [
      note('link', 'Price list https://shree.example/prices'),
      note('plain', 'No link here'),
      note('agent', 'See https://a.example', { kind: 'agent', author: 'atlas' }),
      note('unsafe', 'Look https://evil.example'),
    ]);
    assert.deepEqual(reads.sort(), ['/chats/orders/messages/link/link-preview', '/chats/orders/messages/unsafe/link-preview']);
    const card = article('link').querySelector('a.message-link-card');
    assert.equal(card.getAttribute('href'), 'https://shree.example/prices');
    assert.equal(card.getAttribute('rel'), 'noopener noreferrer');
    assert.match(card.textContent, /^Shree.*Prices 2027.*BWR ply rates$/);
    assert.equal(card.querySelector('img'), null, 'never a picture');
    assert.equal(article('unsafe').querySelector('a.message-link-card'), null, 'only web addresses become links');
    // Rendering again reads nothing more.
    await show(render, [note('link', 'Price list https://shree.example/prices')]);
    assert.equal(reads.length, 2);
  }, { url, fetch: async path => {
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/link-preview')) {
      reads.push(path.replace(`/api/identity/workspaces/${w}`, ''));
      return response({ preview: path.includes('/unsafe/') ? { url: 'javascript:alert(1)', title: 'x', site: 'y' } : { url: 'https://shree.example/prices', title: 'Prices 2027', site: 'Shree', description: 'BWR ply rates' } });
    }
    return response({});
  } });
});
