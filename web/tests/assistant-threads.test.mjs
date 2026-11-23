import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { reactFixture } from './react-fixture.mjs';

// Your assistant working in your chats (schemas 128 and 141): what it suggests starts
// only when you press Start, a chat it works in carries its mark and is listed in
// its own chat, and what it wrote carries its name as a mention, for you alone.
// The header switch and the message box are in assistant-replies.test.mjs.
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

test('a chat your assistant works in carries its mark and is listed in its own chat, where it is turned off', async () => {
  const writes = [];
  const list = [thread('t1', 'running', { mode: 'auto', working: true, instruction: 'Ask about leave in December' }),
    thread('t2', 'running', { mode: 'ask', chat: 'dm-priya', counterpart: 'priya', kind: 'user', name: 'Priya Sharma', draft: { reply: 'r1', state: 'complete', text: 'Friday works.' } }),
    thread('t3', 'stopped', { chat: 'dm-arjun', counterpart: 'arjun', kind: 'user', name: 'Arjun Rao', detail: 'Stopped: Arjun Rao left the workspace' }),
    thread('t4', 'stopped', { chat: 'dm-meera', counterpart: 'meera', kind: 'user', name: 'Meera Iyer' })];
  await reactFixture(async ({ render, click }) => {
    await show(render, view => createElement('div', null,
      ...['dm-hr', 'dm-priya', 'dm-arjun', 'dm-meera'].map(chat => createElement(view.ThreadMark, { key: chat, chat, assistantName: 'Ayati' })),
      createElement(view.AssistantRoom, { assistantName: 'Ayati' })));
    const marks = [...document.querySelectorAll('.chat-thread-mark')];
    assert.deepEqual(marks.map(mark => [mark.getAttribute('aria-label'), mark.classList.contains('auto')]),
      [['Ayati is replying for you', true], ['Ayati asks you first', false]], 'no mark where it is off');
    const rows = [...document.querySelectorAll('.chat-thread-item')];
    assert.match(document.querySelector('.chat-thread-strip h3').textContent, /Working in 2 of your chats/);
    assert.match(rows[0].textContent, /HR Desk.*Writing a reply….*Ask about leave in December/);
    assert.match(rows[1].textContent, /Priya Sharma.*Asks you first · a reply is ready/);
    assert.match(rows[2].textContent, /Arjun Rao.*Stopped: Arjun Rao left the workspace/, 'one it had to stop in says why');
    assert.equal(rows.length, 3, 'a chat you turned it off in is not listed');
    assert.ok(button(rows[0], 'Open'));
    await click(button(rows[0], 'Turn off'));
    assert.deepEqual(writes.at(-1), { path: '/assistant/threads/t1/state', body: { state: 'stopped' } });
    await click(button(rows[2], 'Dismiss'));
    assert.deepEqual(writes.at(-1), { path: '/assistant/threads/t3/state', body: { state: 'deleted' } });
  }, { url, fetch: server(list, writes) });
});

test('what your assistant wrote carries its name as a mention above your own message, for you alone', async () => {
  await reactFixture(async ({ render }) => {
    globalThis.IntersectionObserver = class { observe() {} unobserve() {} disconnect() {} };
    await (await import('../src/workspace-context.ts')).enterWorkspace();
    const { ChatMessage } = await import('../src/react/chat/message.tsx');
    const assisted = { agent: 'ayati', thread: 't1', title: 'Leave in December', by: 'assistant' };
    const mine = { chat: 'dm-hr', author: 'ravi', name: 'Ravi Kumar', kind: 'user', state: 'sent' };
    const list = [
      { ...mine, id: 'm1', content: 'Priya approves it.', at: 2, assisted },
      { ...mine, id: 'm2', content: 'Friday at 3 works.', at: 3, assisted: { agent: 'ayati', title: 'Draft', by: 'assistant', draft: 'r1' } },
      { ...mine, id: 't1', content: 'Hi, I would like leave.', at: 1, assisted },
      { ...mine, id: 'm4', content: 'Thanks!', at: 4, assisted: { ...assisted, by: 'owner' } },
      { ...mine, id: 'm5', author: 'priya', name: 'Priya Sharma', content: 'Noted.', at: 5, assisted }];
    await render(() => createElement('div', null, list.map(message => createElement(ChatMessage, { key: message.id, message, directory, openThread() {} }))));
    const tag = id => document.querySelector(`[data-message-id="${id}"] .chat-assisted`);
    assert.equal(tag('m1').textContent, '@Ayati sent this for you', 'it sent this one itself');
    assert.equal(tag('m1').querySelector('.message-mention').textContent, '@Ayati');
    assert.match(tag('m1').title, /Only you and workspace admins/);
    assert.equal(tag('m1').closest('.chat-message-meta'), null, 'above the message, not beside its time');
    assert.ok(tag('m1').compareDocumentPosition(document.querySelector('[data-message-id="m1"] .message-markdown')) & 4, 'before the text');
    assert.equal(tag('m2').textContent, 'Written with @Ayati', 'a draft you sent yourself');
    assert.equal(tag('t1').textContent, 'Written with @Ayati', 'an opening you started yourself');
    assert.equal(tag('m4'), null, 'what you wrote yourself has no tag');
    assert.equal(tag('m5'), null, 'never on someone else’s message');
  }, { url, fetch: server([], []) });
});
