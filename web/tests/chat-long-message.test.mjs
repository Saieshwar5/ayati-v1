import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement, Fragment } from 'react';
import { reactFixture } from './react-fixture.mjs';

const w = '11111111-1111-4111-8111-111111111111', url = `http://localhost/w/${w}/?view=chat&conversation=orders`;
const session = { user: { id: 'meera', email: 'meera@example.test' }, workspaces: [{ id: w, name: 'Studio', role: 'Member' }] };
const directory = { role: 'Member', users: [{ id: 'meera', name: 'Meera Iyer', title: '', photo: null }, { id: 'ravi', name: 'Ravi Kumar', title: '', photo: null }], agents: [{ id: 'atlas', name: 'Atlas' }] };
const long = 'The container is two days late. '.repeat(60);
const message = (id, author, content, extra = {}) => ({ id, chat: 'orders', author, name: author, kind: 'user', content, state: 'sent', at: 1, ...extra });
const messages = [
  message('mine-long', 'meera', long), message('theirs-long', 'ravi', long),
  message('mine-short', 'meera', 'Dispatch at noon'), message('theirs-short', 'ravi', 'Truck is ready'),
  message('agent', 'atlas', 'Stock is counted.', { kind: 'agent', response_unavailable: true })];
const response = data => ({ ok: true, status: 200, json: async () => data });
const article = id => document.querySelector(`[data-message-id="${id}"]`);
const control = (id, label) => [...article(id).querySelectorAll('button')].find(node => node.getAttribute('aria-label') === label);
const text = id => article(id).querySelector('.chat-text');

/** jsdom lays nothing out: long text is 600px tall, and a closed message clips it at the 192px limit. */
async function show(run) {
  const observers = new Set();
  const previous = { ResizeObserver: globalThis.ResizeObserver, IntersectionObserver: globalThis.IntersectionObserver };
  globalThis.IntersectionObserver = class { observe() {} unobserve() {} disconnect() {} };
  globalThis.ResizeObserver = class { observe() { observers.add(this); } disconnect() { observers.delete(this); } };
  try {
    await reactFixture(async fixture => {
      const element = fixture.dom.window.HTMLElement.prototype, restored = [];
      const tall = node => node.classList.contains('chat-text') && node.textContent.length > 400;
      const open = node => node.classList.contains('chat-text-expanded');
      Object.defineProperty(element, 'scrollHeight', { configurable: true, get() { return tall(this) ? 600 : 0; } });
      Object.defineProperty(element, 'clientHeight', { configurable: true, get() { return tall(this) ? (open(this) ? 600 : 192) : 0; } });
      element.getBoundingClientRect = function () { return { top: 0, height: tall(this) ? (open(this) ? 600 : 192) : 0 }; };
      element.scrollIntoView = function () { restored.push(this); };
      document.head.append(Object.assign(document.createElement('style'), { textContent: '.chat-text { --chat-text-limit:192px; }' }));
      await (await import('../src/workspace-context.ts')).enterWorkspace();
      const { ChatMessage } = await import('../src/react/chat/message.tsx');
      const render = list => fixture.render(() => createElement(Fragment, null, list.map(item => createElement(ChatMessage, { key: item.id, message: item, directory, openThread() {} }))));
      await render(messages);
      await run({ ...fixture, render, restored, observers });
    }, { url, fetch: async () => response(session) });
  } finally { Object.assign(globalThis, previous); }
  return observers;
}

test('long messages from you and from others start closed; short ones have no control', async () => {
  await show(async () => {
    for (const id of ['mine-long', 'theirs-long']) {
      assert.equal(control(id, 'Show full message').getAttribute('aria-expanded'), 'false', id);
      assert.equal(control(id, 'Show full message').getAttribute('aria-controls'), text(id).id);
      assert.equal(text(id).classList.contains('chat-text-long'), true);
      assert.equal(text(id).textContent.trim(), long.trim(), 'the whole text is in the page for copying and screen readers');
    }
    for (const id of ['mine-short', 'theirs-short']) assert.equal(article(id).querySelector('.chat-text-more'), null, id);
  });
});

test("another person's words sit in a bubble; your own message and an agent's reply add none", async () => {
  await show(async () => {
    assert.ok(article('theirs-short').querySelector('.chat-message-bubble .chat-text'));
    assert.equal(article('theirs-short').querySelector('.chat-message-bubble header'), null, 'the name stays above the bubble');
    assert.equal(article('mine-short').querySelector('.chat-message-bubble'), null);
    assert.equal(article('agent').querySelector('.chat-message-bubble'), null);
    assert.match(article('agent').textContent, /Stock is counted/);
  });
});

test('the arrow opens and closes a long message in place, returning to it when closed', async () => {
  await show(async ({ click, restored }) => {
    await click(control('theirs-long', 'Show full message'));
    assert.equal(control('theirs-long', 'Collapse message').getAttribute('aria-expanded'), 'true');
    assert.equal(text('theirs-long').classList.contains('chat-text-expanded'), true);
    assert.equal(control('mine-long', 'Show full message').getAttribute('aria-expanded'), 'false', 'other messages stay closed');
    assert.deepEqual(restored, []);
    await click(control('theirs-long', 'Collapse message'));
    assert.equal(text('theirs-long').classList.contains('chat-text-expanded'), false);
    assert.deepEqual(restored, [text('theirs-long')]);
  });
});

test('reaching hidden text with the keyboard or find-in-page opens the message instead of scrolling inside it', async () => {
  await show(async ({ update, dom }) => {
    const body = text('mine-long');
    await update(() => { body.scrollTop = 80; body.dispatchEvent(new dom.window.Event('scroll')); });
    assert.equal(body.scrollTop, 0);
    assert.ok(control('mine-long', 'Collapse message'));
  });
});

test('an edit that shortens the text removes the control, and leaving the page stops measuring', async () => {
  const observers = await show(async ({ render, observers }) => {
    assert.ok(observers.size > 0);
    await render(messages.map(item => item.id === 'mine-long' ? { ...item, content: 'Shorter now', edited_at: 2 } : item));
    assert.equal(article('mine-long').querySelector('.chat-text-more'), null);
    assert.ok(control('theirs-long', 'Show full message'));
  });
  assert.equal(observers.size, 0);
});
