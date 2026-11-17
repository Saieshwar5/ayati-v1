import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { reactFixture } from './react-fixture.mjs';

// Assistant threads (schema 128): suggested threads start only when you press
// Start, a running thread can be stopped where it runs, a new thread is sent
// once, and only your own messages carry the "written by" mark.
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
      if (writes.length <= fail) return response({ error: 'You already have a running thread with HR Desk.' }, 409);
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

test('a suggested thread shows in your assistant’s chat and starts once when you press Start', async () => {
  const writes = [];
  await reactFixture(async ({ render, update }) => {
    await show(render, view => createElement(view.ThreadBanner, { chat: 'room', assistantName: 'Ayati' }));
    const banner = document.querySelector('.chat-thread-banner.suggested');
    assert.match(banner.textContent, /Ayati suggests a thread with HR Desk.*Leave in December/);
    assert.match(banner.textContent, /22 to 26 December/);
    assert.match(banner.textContent, /sent as your message/);
    assert.equal(writes.length, 0, 'nothing is sent by showing it');
    const start = button(banner, 'Start thread');
    await update(() => { start.click(); start.click(); });
    assert.deepEqual(writes, [{ path: '/assistant/threads/t1/state', body: { state: 'running' } }], 'one request despite a double click');
  }, { url, fetch: server([thread('t1', 'suggested')], writes) });
});

test('a running thread says so where it runs and can be stopped there; a stopped one resumes or is deleted', async () => {
  const writes = [];
  await reactFixture(async ({ render, click }) => {
    await show(render, view => createElement('div', null,
      createElement(view.ThreadBanner, { chat: 'dm-hr', assistantName: 'Ayati' }),
      createElement(view.ThreadRows, { selected: 'dm-hr', manage() {} })));
    const banner = document.querySelector('.chat-thread-banner.running');
    assert.match(banner.textContent, /Ayati is answering HR Desk for you.*Leave in December.*writing a reply/);
    const row = document.querySelector('.chat-thread-row');
    assert.match(row.textContent, /Leave in December.*with HR Desk/);
    assert.match(row.getAttribute('href'), /conversation=dm-hr/);
    assert.equal(row.getAttribute('aria-current'), 'page');
    await click(button(banner, 'Stop'));
    assert.deepEqual(writes.at(-1), { path: '/assistant/threads/t1/state', body: { state: 'stopped' } });
  }, { url, fetch: server([thread('t1', 'running', { working: true })], writes) });
  await reactFixture(async ({ render, click }) => {
    await show(render, view => createElement(view.ThreadBanner, { chat: 'dm-hr', assistantName: 'Ayati' }));
    const banner = document.querySelector('.chat-thread-banner.stopped');
    assert.match(banner.textContent, /Thread stopped.*You stopped it/);
    await click(button(banner, 'Resume'));
    assert.deepEqual(writes.at(-1).body, { state: 'running' });
    await click(button(document.querySelector('.chat-thread-banner.stopped'), 'Delete'));
    assert.deepEqual(writes.at(-1).body, { state: 'deleted' });
  }, { url, fetch: server([thread('t1', 'stopped', { detail: 'You stopped it.' })], writes) });
});

test('a new thread needs who, what and the first message; a failed start keeps your words and retries with its ID', async () => {
  const writes = [];
  await reactFixture(async ({ render, click, change }) => {
    await show(render, view => createElement(view.ThreadsDialog, { directory, assistantName: 'Ayati', close() {} }));
    const dialog = () => document.querySelector('dialog');
    assert.match(dialog().textContent, /No threads yet/);
    await click(button(dialog(), 'New thread'));
    const options = [...dialog().querySelectorAll('option')].map(node => node.textContent);
    assert.deepEqual(options, ['Choose a person or an agent', 'HR Desk', 'Priya Sharma'], 'no assistants and not yourself');
    assert.equal(button(dialog(), 'Start thread').disabled, true);
    await change(dialog().querySelector('select'), 'agent:hr');
    await change(dialog().querySelector('input'), 'Leave in December');
    await change(dialog().querySelector('textarea'), 'Hi, I would like leave.');
    await click(button(dialog(), 'Start thread'));
    assert.match(dialog().textContent, /already have a running thread/);
    assert.equal(dialog().querySelector('textarea').value, 'Hi, I would like leave.');
    await click(button(dialog(), 'Start thread'));
    assert.equal(writes.length, 2);
    assert.equal(writes[1].body.id, writes[0].body.id, 'the same thread keeps its ID');
    assert.deepEqual({ ...writes[1].body, id: undefined }, { id: undefined, kind: 'agent', counterpart: 'hr', title: 'Leave in December', opening: 'Hi, I would like leave.' });
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
    assert.match(mark('m1').textContent, /Written by Ayati · Leave in December/);
    assert.equal(mark('m2'), null, 'what you wrote yourself has no mark');
    assert.equal(mark('m3'), null, 'never on someone else’s message');
  }, { url, fetch: server([], []) });
});
