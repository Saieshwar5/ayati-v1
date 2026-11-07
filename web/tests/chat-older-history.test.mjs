import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=chat`;
const session = { user: { id: 'owner', email: 'owner@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Admin' }] };
const chat = { id: 'ops', name: 'ops', kind: 'channel', description: '', users: ['owner'], agents: [] };
const directory = { role: 'Admin', users: [{ id: 'owner', name: 'Ananya' }], agents: [] };
const message = (id, content, sequence) => ({ id, chat: 'ops', author: 'owner', kind: 'user', content, state: 'sent', sequence, revision: 1, at: sequence, thread: null, attachments: [] });
const page = (messages, next_before) => ({ messages, agents: [], next_before });
const pages = {
  latest: page([message('latest', 'Latest message', 500)], 400),
  400: page([message('older', 'Older message', 300)], 200),
  200: page([message('first', 'First message', 100)], null),
};
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const parse = path => new URL(path, 'http://localhost');
const isHistory = path => parse(path).pathname.endsWith('/chats/ops/messages');
const ids = list => [...list.querySelectorAll('[data-message-id]')].map(node => node.dataset.messageId);

async function openPane(render) {
  const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
  const { ConversationPane } = await import('../src/react/chat/conversation.tsx');
  await render(ConversationPane, { chat, directory, back: () => {} });
  return document.querySelector('[role=log]');
}

// jsdom has no layout, so the list always reports the top: every scroll is "near the top".
const scroller = (dom, update, list) => () => update(() => list.dispatchEvent(new dom.window.Event('scroll')));

test('scrolling near the top loads each older page once, in order, and stops at the first message', async () => {
  const asked = []; let release;
  await reactFixture(async ({ dom, render, update, button }) => {
    const list = await openPane(render);
    assert.deepEqual(ids(list), ['latest']);
    const scroll = scroller(dom, update, list);
    await scroll(); await scroll();
    assert.deepEqual(asked, ['400'], 'one read while the first is still loading');
    assert.equal(button('Loading older messages…').disabled, true);
    await update(() => release());
    assert.deepEqual(ids(list), ['older', 'latest']);
    await scroll(); await scroll();
    assert.deepEqual(asked, ['400', '200'], 'each page is asked for once');
    assert.deepEqual(ids(list), ['first', 'older', 'latest']);
    assert.equal(button('Load older messages'), undefined, 'nothing older remains');
    await scroll();
    assert.deepEqual(asked, ['400', '200']);
  }, { url, fetch: async path => {
    if (!isHistory(path)) return response(session);
    const before = parse(path).searchParams.get('before');
    if (!before) return response(pages.latest);
    asked.push(before);
    if (before === '400' && !release) return new Promise(resolve => { release = () => resolve(response(pages[400])); });
    return response(pages[before]);
  } });
});

test('a failed older read waits for the button instead of retrying on every scroll', async () => {
  const asked = []; let failing = true;
  await reactFixture(async ({ dom, render, update, button, click }) => {
    const list = await openPane(render);
    const scroll = scroller(dom, update, list);
    await scroll();
    assert.deepEqual(asked, ['400']);
    assert.match(document.querySelector('[role=alert]').textContent, /History is unavailable/);
    failing = false;
    await scroll(); await scroll();
    assert.deepEqual(asked, ['400'], 'scrolling does not retry a failed read');
    await click(button('Load older messages'));
    assert.deepEqual(asked, ['400', '400']);
    assert.deepEqual(ids(list), ['older', 'latest']);
  }, { url, fetch: async path => {
    if (!isHistory(path)) return response(session);
    const before = parse(path).searchParams.get('before');
    if (!before) return response(pages.latest);
    asked.push(before);
    return failing ? response({ error: 'History is unavailable' }, 503) : response(pages[before]);
  } });
});

test('a live reset forgets loaded older pages, so the next read continues from the reloaded latest page', async () => {
  const asked = []; let stream, stopLive;
  const original = globalThis.EventSource;
  globalThis.EventSource = class { constructor() { this.listeners = new Map(); stream = this; } addEventListener(name, fn) { this.listeners.set(name, fn); } close() {} };
  try {
    await reactFixture(async ({ dom, render, update }) => {
      const list = await openPane(render);
      const { startLive } = await import('../src/react/chat/live.ts');
      await update(() => { stopLive = startLive(); });
      const scroll = scroller(dom, update, list);
      await scroll();
      assert.deepEqual(ids(list), ['older', 'latest']);
      await update(() => stream.listeners.get('changes')({ data: JSON.stringify({ cursor: 8, reset: true, events: [] }) }));
      assert.deepEqual(ids(list), ['latest']);
      await scroll();
      assert.deepEqual(asked, ['400', '400'], 'no gap: the old cursor is not reused after a reset');
      await update(() => stopLive?.());
    }, { url, fetch: async path => {
      if (parse(path).pathname.endsWith('/message-capabilities')) return response({ live: true });
      if (!isHistory(path)) return response(session);
      const before = parse(path).searchParams.get('before');
      if (!before) return response(pages.latest);
      asked.push(before);
      return response(pages[before]);
    } });
  } finally { globalThis.EventSource = original; }
});
