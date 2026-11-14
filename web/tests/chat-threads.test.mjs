import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';
import { outboxStorage } from './outbox-storage-fixture.mjs';

const w = '11111111-1111-4111-8111-111111111111';
const session = { user: { id: 'owner', email: 'owner@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Admin' }] };
const chat = { id: 'orders', name: 'orders', kind: 'channel', users: ['owner', 'meera'], agents: [] };
const directory = { role: 'Admin', users: [{ id: 'owner', name: 'Ravi' }, { id: 'meera', name: 'Meera' }], agents: [] };
const today = Math.floor(new Date(2026, 10, 14, 10, 52).getTime() / 1000);
const message = (id, content, extra = {}) => ({ id, chat: 'orders', author: 'meera', name: 'Meera', kind: 'user', content, state: 'sent', sequence: 1, revision: 1, at: today - 600, thread: null, attachments: [], ...extra });
const page = { agents: [], next_before: null, messages: [
  message('root', 'Which truck goes to Pune?', { thread_replies: 4, thread_last: today }),
  message('r1', 'The 4 PM one', { thread: 'root', sequence: 2 }),
  message('shared', 'Truck leaves at 4 PM, everyone', { thread: 'root', broadcast: true, sequence: 3 }),
  message('quiet', 'Only in the thread', { thread: 'root', sequence: 4 }),
] };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const settle = async update => { for (let i = 0; i < 6; i++) await update(); };

async function open(run, { details = { open: false, toggle() {}, show() {}, review() {} } } = {}) {
  const previous = { indexedDB: globalThis.indexedDB, sessionStorage: globalThis.sessionStorage, IntersectionObserver: globalThis.IntersectionObserver };
  const sent = [];
  globalThis.indexedDB = outboxStorage();
  globalThis.IntersectionObserver = class { observe() {} unobserve() {} disconnect() {} };
  try {
    await reactFixture(async fixture => {
      globalThis.sessionStorage = fixture.dom.window.sessionStorage;
      const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
      const { ConversationPane } = await import('../src/react/chat/conversation.tsx');
      const rerender = props => fixture.render(ConversationPane, { chat, directory, back: () => {}, details, ...props });
      await rerender();
      await settle(fixture.update);
      await run({ ...fixture, sent, rerender });
    }, { url: `http://localhost/w/${w}/?view=chat`, fetch: async (path, options = {}) => {
      path = new URL(path, 'http://localhost').pathname;
      if (path.endsWith('/session')) return response(session);
      if (path.endsWith('/messages') && options.method === 'POST') {
        const body = JSON.parse(options.body); sent.push(body);
        return response({ ok: true, message: { ...message(body.id, body.content), author: 'owner', thread: body.thread ?? null, broadcast: Boolean(body.broadcast), sequence: 9 } });
      }
      if (path.endsWith('/messages')) return response(page);
      return response({ ok: true });
    } });
  } finally { Object.assign(globalThis, previous); }
}

test('a thread shows its reply count and last reply from the server, and replies sent here appear in the conversation', async () => {
  await open(async () => {
    const log = document.querySelector('[role=log][aria-label=Messages]');
    const shown = [...log.querySelectorAll('[data-message-id]')].map(node => node.dataset.messageId);
    assert.deepEqual(shown, ['root', 'shared'], 'only replies also sent to the conversation leave the thread');
    // The count is the server's (4), not the three replies this page happens to hold.
    const summary = log.querySelector('.chat-thread-count');
    assert.match(summary.textContent, /^4 replies · last \d{1,2}:\d{2}( [AP]M)?$/);
    assert.match(summary.getAttribute('aria-label'), /Open thread$/);
    const shared = log.querySelector('[data-message-id=shared]');
    assert.match(shared.querySelector('.chat-in-thread').textContent, /Replied in a thread: .*Which truck goes to Pune\?.*View thread/);
    assert.equal(shared.querySelector('.chat-thread-count'), null, 'a reply has no thread of its own');
  });
});

test('the thread opens beside the conversation and a reply can also go to the channel', async () => {
  await open(async ({ click, change, submit, update, button, sent }) => {
    await click([...document.querySelectorAll('.chat-in-thread button')][0]);
    const panel = document.querySelector('aside.chat-thread-panel');
    assert.ok(panel, 'a side panel, not a modal dialog');
    assert.equal(document.querySelector('dialog[open]'), null);
    assert.equal(panel.getAttribute('aria-label'), 'Thread');
    assert.match(panel.querySelector('h2').textContent, /Thread#orders/);
    assert.deepEqual([...panel.querySelectorAll('[role=log] [data-message-id]')].map(node => node.dataset.messageId), ['root', 'r1', 'shared', 'quiet']);
    assert.ok(document.querySelector('section.chat-conversation [role=log]'), 'the conversation stays on screen');

    const box = () => panel.querySelector('textarea[aria-label="Reply in thread"]');
    const also = () => [...panel.querySelectorAll('label')].find(label => label.textContent.includes('Also send to #orders')).querySelector('input');
    await change(box(), 'Loading now');
    await submit(panel.querySelector('.message-composer'));
    await settle(update);
    assert.equal(sent.length, 1);
    assert.equal('broadcast' in sent[0], false, 'an ordinary reply keeps its earlier payload');

    await click(also());
    assert.equal(also().checked, true);
    await change(box(), 'Everyone: truck leaves at 4');
    await submit(panel.querySelector('.message-composer'));
    await settle(update);
    assert.deepEqual([sent[1].thread, sent[1].broadcast], ['root', true]);
    assert.equal(also().checked, false, 'the choice is for one reply');

    await click(button('Close Thread'));
    assert.equal(document.querySelector('.chat-thread-panel'), null);
  });
});

test('the thread and Details take turns beside the conversation', async () => {
  let toggles = 0;
  const details = { open: true, toggle() { toggles++; }, show() {}, review() {} };
  await open(async ({ click, rerender }) => {
    await click(document.querySelector('.chat-thread-count'));
    assert.equal(toggles, 1, 'opening a thread closes Details');
    assert.equal(document.querySelector('.chat-thread-panel'), null, 'hidden while Details is still open');
    await rerender({ details: { ...details, open: false } });
    assert.ok(document.querySelector('.chat-thread-panel'));
  }, { details });
});

test('reply times say the time today and the date before', async () => {
  const { replyTime } = await import('../src/react/chat/thread-summary.tsx');
  const now = new Date(2026, 10, 14, 18, 0);
  assert.match(replyTime(today, now), /^\d{1,2}:\d{2}( [AP]M)?$/);
  assert.match(replyTime(today - 86_400 * 2, now), /12/);
});
