import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { reactFixture } from './react-fixture.mjs';

// Your assistant replying for you (schemas 128 and 136): what it suggests starts
// only when you press Start, a chat it replies in says so in its header and can
// be stopped there, a chat is handed over from a message with one instruction,
// and only your own messages carry its mark.
const w = '11111111-1111-4111-8111-111111111111', url = `http://localhost/w/${w}/?view=chat&conversation=room`;
const session = { user: { id: 'ravi', email: 'ravi@example.test' }, workspaces: [{ id: w, name: 'Northwind', role: 'Member' }] };
const directory = { role: 'Member',
  users: [{ id: 'ravi', name: 'Ravi Kumar' }, { id: 'priya', name: 'Priya Sharma' }],
  agents: [{ id: 'ayati', name: 'Ayati', audience: 'only_me', yours: true, assistant: true }, { id: 'hr', name: 'HR Desk', audience: 'everyone' },
    { id: 'hers', name: 'Ayati', audience: 'only_me', assistant: true }] };
const thread = (id, state, more = {}) => ({ id, kind: 'agent', counterpart: 'hr', name: 'HR Desk', title: 'Leave in December', opening: 'Hi, I would like leave from 22 to 26 December.',
  chat: state === 'suggested' ? null : 'dm-hr', state, detail: '', sent: 1, origin: 'assistant', updated_ms: 1, working: false, ...more });
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
function server(list, writes, { fail = 0 } = {}) {
  return async (path, options = {}) => {
    if (options.method === 'POST') {
      writes.push({ path: path.replace(`/api/identity/workspaces/${w}`, ''), body: JSON.parse(options.body) });
      if (writes.length <= fail) return response({ error: 'Your assistant is already replying to Priya Sharma here.' }, 409);
      return response({ ok: true, chat: 'dm-hr' });
    }
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/assistant/threads')) return response({ threads: list });
    return response({});
  };
}
async function show(render, part) {
  await (await import('../src/workspace-context.ts')).enterWorkspace();
  const model = await import('../src/react/chat/assistant-threads-model.ts');
  const view = await import('../src/react/chat/assistant-threads.tsx');
  function Screen() {
    const threads = model.useAssistantThreads('ayati', 'room');
    return createElement(model.AssistantThreads.Provider, { value: threads }, part(view));
  }
  await render(Screen);
}
const button = (node, label) => [...node.querySelectorAll('button')].find(b => b.textContent.trim() === label);

test('what your assistant suggests shows in its own chat and starts once when you press Start', async () => {
  const writes = [];
  await reactFixture(async ({ render, update }) => {
    await show(render, view => createElement(view.AssistantRoom, { assistantName: 'Ayati' }));
    const banner = document.querySelector('.chat-thread-banner.suggested');
    assert.match(banner.textContent, /Ayati suggests talking to HR Desk for you.*Leave in December/);
    assert.match(banner.textContent, /22 to 26 December/);
    assert.match(banner.textContent, /sent as your message/);
    assert.equal(document.querySelector('.chat-thread-strip'), null, 'nothing is handed over yet');
    assert.equal(writes.length, 0, 'nothing is sent by showing it');
    const start = button(banner, 'Start');
    await update(() => { start.click(); start.click(); });
    assert.deepEqual(writes, [{ path: '/assistant/threads/t1/state', body: { state: 'running' } }], 'one request despite a double click');
  }, { url, fetch: server([thread('t1', 'suggested')], writes) });
});

test('a chat your assistant replies in says so in its header, the list and its own chat; stop, resume and delete', async () => {
  const writes = [];
  await reactFixture(async ({ render, click }) => {
    await show(render, view => createElement('div', null,
      createElement(view.ThreadPill, { chat: 'dm-hr', assistantName: 'Ayati' }),
      createElement(view.ThreadMark, { chat: 'dm-hr', assistantName: 'Ayati' }),
      createElement(view.ThreadMark, { chat: 'dm-priya', assistantName: 'Ayati' }),
      createElement(view.AssistantRoom, { assistantName: 'Ayati' })));
    const pill = document.querySelector('.chat-thread-pill.running');
    assert.match(pill.querySelector('summary').textContent, /Ayati is writing…/);
    assert.match(pill.querySelector('.chat-thread-about').textContent, /What you asked.*Ask about leave in December/);
    const marks = [...document.querySelectorAll('.chat-thread-mark')];
    assert.deepEqual(marks.map(mark => mark.getAttribute('aria-label')), ['Ayati is replying for you'], 'only on the chat it replies in');
    const strip = document.querySelector('.chat-thread-strip');
    assert.match(strip.textContent, /Replying for you in 1 chat.*HR Desk.*Writing a reply/);
    assert.ok(button(strip, 'Open'));
    await click(button(pill, 'Stop'));
    assert.deepEqual(writes.at(-1), { path: '/assistant/threads/t1/state', body: { state: 'stopped' } });
  }, { url, fetch: server([thread('t1', 'running', { working: true, instruction: 'Ask about leave in December' })], writes) });
  await reactFixture(async ({ render, click }) => {
    await show(render, view => createElement('div', null,
      createElement(view.ThreadPill, { chat: 'dm-hr', assistantName: 'Ayati' }), createElement(view.ThreadMark, { chat: 'dm-hr', assistantName: 'Ayati' })));
    const pill = () => document.querySelector('.chat-thread-pill.stopped');
    assert.match(pill().textContent, /Ayati stopped.*You stopped it/);
    assert.equal(document.querySelector('.chat-thread-mark').getAttribute('aria-label'), 'Ayati stopped replying here');
    await click(button(pill(), 'Resume'));
    assert.deepEqual(writes.at(-1).body, { state: 'running' });
    await click(button(pill(), 'Delete'));
    assert.deepEqual(writes.at(-1).body, { state: 'deleted' });
  }, { url, fetch: server([thread('t1', 'stopped', { detail: 'You stopped it.' })], writes) });
});

test('a chat is handed over from the other side’s message with one instruction; a failed start keeps your words and its ID', async () => {
  const writes = [];
  await reactFixture(async ({ render, click, change }) => {
    globalThis.IntersectionObserver = class { observe() {} unobserve() {} disconnect() {} };
    await (await import('../src/workspace-context.ts')).enterWorkspace();
    const { ChatMessage } = await import('../src/react/chat/message.tsx');
    const list = [
      { id: 'm1', chat: 'dm-priya', author: 'priya', name: 'Priya Sharma', kind: 'user', content: 'Any update on the funding?', state: 'sent', at: 2 },
      { id: 'm2', chat: 'dm-priya', author: 'ravi', name: 'Ravi Kumar', kind: 'user', content: 'Let me check.', state: 'sent', at: 3 }];
    // The conversation offers it only on the other side's messages.
    await render(() => createElement('div', null, list.map(message => createElement(ChatMessage, { key: message.id, message, directory, openThread() {}, handOver: message.author === 'priya' ? 'Ayati' : undefined }))));
    const tool = id => document.querySelector(`[data-message-id="${id}"] .chat-takeover-tool`);
    assert.equal(tool('m2'), null, 'not on your own message');
    assert.equal(tool('m1').getAttribute('aria-label'), 'Let Ayati take over from here');
    await click(tool('m1'));
    const dialog = () => document.querySelector('dialog');
    assert.match(dialog().textContent, /Let Ayati take over from here.*Priya Sharma.*Any update on the funding\?/);
    assert.match(dialog().textContent, /replies to Priya Sharma as you, starting with this message, until you stop it/);
    assert.equal(button(dialog(), 'Start').disabled, true, 'it needs to know what to do');
    await change(dialog().querySelector('textarea'), ' Tell her we are close. ');
    await click(button(dialog(), 'Start'));
    assert.match(dialog().textContent, /already replying/);
    assert.equal(dialog().querySelector('textarea').value, ' Tell her we are close. ');
    await click(button(dialog(), 'Start'));
    assert.equal(writes.length, 2);
    assert.equal(writes[1].body.id, writes[0].body.id, 'the same hand-over keeps its ID');
    assert.deepEqual({ ...writes[1], body: { ...writes[1].body, id: undefined } },
      { path: '/assistant/take-over', body: { id: undefined, chat: 'dm-priya', message: 'm1', instruction: 'Tell her we are close.' } });
    assert.equal(dialog(), null, 'closed once started');
  }, { url, fetch: server([], writes, { fail: 1 }) });
});

test('only your own message shows that your assistant wrote it', async () => {
  await reactFixture(async ({ render }) => {
    globalThis.IntersectionObserver = class { observe() {} unobserve() {} disconnect() {} };
    await (await import('../src/workspace-context.ts')).enterWorkspace();
    const { ChatMessage } = await import('../src/react/chat/message.tsx');
    const assisted = { agent: 'ayati', thread: 't1', title: 'Leave in December', by: 'assistant' };
    const list = [
      { id: 'm1', chat: 'dm-hr', author: 'ravi', name: 'Ravi Kumar', kind: 'user', content: 'Priya approves it.', state: 'sent', at: 2, assisted },
      { id: 'm2', chat: 'dm-hr', author: 'ravi', name: 'Ravi Kumar', kind: 'user', content: 'Thanks!', state: 'sent', at: 3, assisted: { ...assisted, by: 'owner' } },
      { id: 'm3', chat: 'dm-hr', author: 'priya', name: 'Priya Sharma', kind: 'user', content: 'Noted.', state: 'sent', at: 4, assisted }];
    await render(() => createElement('div', null, list.map(message => createElement(ChatMessage, { key: message.id, message, directory, openThread() {} }))));
    const mark = id => document.querySelector(`[data-message-id="${id}"] .chat-assisted`);
    assert.equal(mark('m1').textContent, 'Ayati');
    assert.match(mark('m1').title, /Written by Ayati · Leave in December/);
    assert.ok(mark('m1').closest('.chat-message-meta'), 'beside its time');
    assert.equal(mark('m2'), null, 'what you wrote yourself has no mark');
    assert.equal(mark('m3'), null, 'never on someone else’s message');
  }, { url, fetch: server([], []) });
});
