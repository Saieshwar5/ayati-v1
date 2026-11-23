import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { reactFixture } from './react-fixture.mjs';
import { outboxStorage } from './outbox-storage-fixture.mjs';

// What your assistant does in a chat (schema 141): the switch in a direct chat's
// header, a reply that waits in the draft card on Ask me first, and a reply to
// one message started from that message. Asking by mention itself is in
// assistant-ask.test.mjs. Nothing it writes here is posted until you send it.
const w = '11111111-1111-4111-8111-111111111111', url = `http://localhost/w/${w}/?view=chat&conversation=dm-priya`;
const session = { user: { id: 'ravi', email: 'ravi@example.test' }, workspaces: [{ id: w, name: 'Northwind', role: 'Member' }] };
const directory = { role: 'Member',
  users: [{ id: 'ravi', name: 'Ravi Kumar' }, { id: 'priya', name: 'Priya Sharma' }],
  agents: [{ id: 'ayati', name: 'Ayati', audience: 'only_me', yours: true, assistant: true }] };
const chat = { id: 'dm-priya', name: 'Direct message', kind: 'direct', description: '', users: ['ravi', 'priya'], agents: [] };
const said = (id, author, content, sequence) => ({ id, chat: 'dm-priya', author, name: author === 'ravi' ? 'Ravi Kumar' : 'Priya Sharma', kind: 'user', content, state: 'sent', sequence, revision: 1, at: sequence, thread: null, attachments: [] });
const history = [said('m1', 'priya', 'Can we meet on Friday at 3?', 1), said('m2', 'ravi', 'Let me check.', 2), said('m3', 'priya', 'Or Thursday?', 3)];
const thread = (more = {}) => ({ id: 't1', kind: 'user', counterpart: 'priya', name: 'Priya Sharma', title: 'Chat with Priya Sharma', opening: '', chat: 'dm-priya',
  state: 'running', mode: 'ask', detail: '', sent: 0, origin: 'owner', updated_ms: 1, working: false, ...more });
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const button = (node, label) => [...node.querySelectorAll('button')].find(b => b.textContent.trim() === label || b.getAttribute('aria-label') === label);

/** Opens Ravi's chat with Priya with his assistant's threads. `writes` records every
 * POST; `fail` refuses that many first; the draft timer is run by hand with `tick`. */
async function withChat({ threads = [], writes = [], fail = 0, draft }, run) {
  const original = { EventSource: globalThis.EventSource, IntersectionObserver: globalThis.IntersectionObserver, indexedDB: globalThis.indexedDB, sessionStorage: globalThis.sessionStorage, requestAnimationFrame: globalThis.requestAnimationFrame };
  globalThis.EventSource = class { addEventListener() {} close() {} };
  globalThis.IntersectionObserver = class { observe() {} unobserve() {} disconnect() {} };
  globalThis.indexedDB = outboxStorage();
  globalThis.requestAnimationFrame = fn => { fn(); return 1; };
  try {
    await reactFixture(async fixture => {
      let tick; fixture.dom.window.setInterval = fn => { tick = fn; return 1; }; fixture.dom.window.clearInterval = () => {};
      globalThis.sessionStorage = fixture.dom.window.sessionStorage;
      const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
      if (draft) sessionStorage.setItem(context.storageScope('relay.chat.draft.dm-priya'), draft);
      const box = await import('../src/react/chat/outbox.ts');
      const model = await import('../src/react/chat/assistant-threads-model.ts');
      const { ConversationPane } = await import('../src/react/chat/conversation.tsx');
      function Screen() {
        const value = model.useAssistantThreads('ayati', 'room');
        return createElement(model.AssistantThreads.Provider, { value }, createElement(ConversationPane, { chat, directory, back() {} }));
      }
      await fixture.render(Screen);
      const until = async condition => { for (let i = 0; i < 60 && !condition(); i++) await fixture.update(); assert.ok(condition()); };
      await until(() => document.querySelector('[data-message-id="m3"]'));
      try { await run({ ...fixture, until, tick: () => fixture.update(() => { void tick(); }), input: () => document.querySelector('.message-composer textarea'), notice: () => document.querySelector('.composer-notice')?.textContent ?? '' }); }
      finally { await fixture.update(() => box.stopOutbox()); }
    }, { url, fetch: async (path, options = {}) => {
      const at = new URL(path, 'http://localhost').pathname.replace(`/api/identity/workspaces/${w}`, '');
      if (options.method === 'POST') {
        writes.push({ path: at, body: JSON.parse(options.body) });
        if (writes.length <= fail) return response({ error: 'Your assistant is paused.' }, 409);
        if (at === '/chats/dm-priya/messages') return response({ ok: true, message: { ...said(writes.at(-1).body.id, 'ravi', writes.at(-1).body.content, 9) } });
        return response({ ok: true, reply: 'r1', thread: 't1' });
      }
      if (at.endsWith('/message-capabilities')) return response({ live: true });
      if (at === '/chats/dm-priya/messages') return response({ agents: [], next_before: null, messages: history });
      if (at === '/assistant/threads') return response({ threads });
      if (at === '/assistant/drafts/r1') return response({ state: 'complete', text: 'Thursday at 3 works for me.' });
      return response(session);
    } });
  } finally { Object.assign(globalThis, original); }
}

test('the header switch turns your assistant on once, and replying for you starts only from its own button', async () => {
  const writes = [];
  await withChat({ writes, fail: 1 }, async ({ click, change, update }) => {
    const toggle = () => document.querySelector('.chat-replies > button');
    assert.equal(toggle().getAttribute('aria-label'), 'Ayati in this chat: Off');
    assert.equal(toggle().textContent, '', 'a quiet mark while it is off');
    await click(toggle());
    const menu = () => document.querySelector('.chat-replies-menu');
    const choice = label => [...menu().querySelectorAll('[role=radio]')].find(item => item.textContent.startsWith(label));
    assert.deepEqual([...menu().querySelectorAll('[role=radio]')].map(item => item.getAttribute('aria-checked')), ['true', 'false', 'false']);
    // Replying for you is not started by choosing it: it asks first, and says what it means.
    await click(choice('Reply for me'));
    assert.equal(writes.length, 0);
    assert.match(menu().textContent, /go to Priya Sharma as your messages, without you reading them first/);
    await change(menu().querySelector('textarea'), ' Settle a time for Friday. ');
    const start = button(menu(), 'Start replying');
    await update(() => { start.click(); start.click(); });
    assert.equal(writes.length, 1, 'one request despite a double click');
    assert.match(menu().textContent, /Your assistant is paused/);
    assert.equal(menu().querySelector('textarea').value, ' Settle a time for Friday. ', 'a refused start keeps your words');
    await click(button(menu(), 'Start replying'));
    assert.equal(writes[1].body.id, writes[0].body.id, 'the same choice keeps its ID');
    assert.deepEqual({ ...writes[1], body: { ...writes[1].body, id: undefined } },
      { path: '/assistant/replies', body: { id: undefined, chat: 'dm-priya', mode: 'auto', instruction: 'Settle a time for Friday.' } });
    assert.equal(menu(), null, 'closed once started');
    // The other two choices apply at once.
    await click(toggle()); await click(choice('Ask me first'));
    assert.deepEqual({ ...writes[2].body, id: undefined }, { id: undefined, chat: 'dm-priya', mode: 'ask' });
  });
  await withChat({ writes, threads: [thread({ mode: 'auto', sent: 2, instruction: 'Settle a time for Friday.', anchor: 'm3', detail: '' })] }, async ({ click }) => {
    const toggle = document.querySelector('.chat-replies > button');
    assert.equal(toggle.textContent, 'Ayati is replying');
    assert.equal(document.querySelector('.chat-took-over').nextElementSibling.dataset.messageId, 'm3', 'where its replies for you begin');
    await click(toggle);
    assert.match(document.querySelector('.chat-replies-note').textContent, /What you asked: Settle a time for Friday\..*2 messages sent as you/);
    await click([...document.querySelectorAll('.chat-replies-menu [role=radio]')][0]);
    assert.deepEqual({ ...writes.at(-1).body, id: undefined }, { id: undefined, chat: 'dm-priya', mode: 'off' });
  });
});

test('on Ask me first the reply waits in the draft card: sent as you with its record, moved into the box, or discarded', async () => {
  const waiting = () => [thread({ draft: { reply: 'r9', state: 'complete', text: 'Thursday works for me.' } })];
  const card = () => document.querySelector('.assistant-ask');
  let writes = [];
  await withChat({ threads: waiting(), writes, draft: 'I was about to say' }, async ({ input, click, until }) => {
    await until(() => card());
    assert.match(card().textContent, /Draft from Ayati.*Only you see this · Will be sent as you to Priya Sharma.*Thursday works for me\./);
    assert.equal(input().value, 'I was about to say', 'your message box stays yours');
    assert.equal(document.querySelector('.chat-replies > button').textContent, 'Ayati asks you first');
    assert.equal(writes.length, 0, 'nothing is sent by showing it');
    await click(button(card(), 'Send as me'));
    await until(() => writes.some(write => write.path === '/assistant/drafts/r9/used'));
    const posted = writes.find(write => write.path === '/chats/dm-priya/messages');
    assert.equal(posted.body.content, 'Thursday works for me.');
    assert.deepEqual(writes.find(write => write.path === '/assistant/drafts/r9/used').body, { message: posted.body.id }, 'the private record of who wrote it');
    assert.equal(card(), null, 'not offered again');
  });
  writes = [];
  await withChat({ threads: waiting(), writes }, async ({ input, notice, click, until }) => {
    await until(() => card());
    await click(button(card(), 'Edit'));
    assert.equal(input().value, 'Thursday works for me.');
    assert.match(notice(), /Draft from Ayati.*Edit it, then send/);
    assert.equal(card(), null);
    await click(button(document, 'Discard'));
    assert.equal(input().value, '');
    await click(button(card(), 'Discard'));
    assert.deepEqual(writes, [{ path: '/assistant/threads/t1/draft/discard', body: {} }], 'discarding tells the thread');
    assert.equal(card(), null);
  });
});

test('a reply to one message is started from that message, on the other side’s messages only', async () => {
  const writes = [];
  await withChat({ writes }, async ({ input, click, submit, until }) => {
    const tool = id => document.querySelector(`[data-message-id="${id}"] .chat-assistant-tool`);
    assert.equal(tool('m2'), null, 'not on your own message');
    assert.equal(tool('m1').getAttribute('aria-label'), 'Reply with @Ayati');
    await click(tool('m1'));
    assert.equal(input().value, '@Ayati ');
    assert.match(document.querySelector('.chat-replying').textContent, /Replying to Priya Sharma.*Can we meet on Friday at 3\?/);
    assert.equal(document.querySelector('.message-composer button[type=submit]').textContent, 'Ask Ayati');
    assert.equal(writes.length, 0, 'nothing is asked until you send');
    await submit(document.querySelector('form.message-composer'));
    await until(() => writes.length === 1);
    assert.equal(writes[0].path, '/assistant/drafts', 'it asks your assistant and posts nothing');
    assert.equal(writes[0].body.message, 'm1', 'about the message you chose');
    assert.equal(writes[0].body.chat, 'dm-priya');
  });
});
