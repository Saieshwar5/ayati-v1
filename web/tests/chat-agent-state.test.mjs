import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

// An agent's state in Chat (agent-state.tsx): the dot where a person's online dot
// sits, the line under its name, and a quiet bar in place of the message box while
// it takes no messages. The header's name opens the profile; owners get settings.
const w = '11111111-1111-4111-8111-111111111111';
const at = conversation => `http://localhost/w/${w}/?view=chat&conversation=${conversation}`;
const iris = { id: 'iris', name: 'Iris', role: 'Answers customer questions', audience: 'everyone', manager: 'Ananya Rao' };
const dm = { id: 'dm-iris', name: 'Iris', kind: 'direct', description: '', users: ['dev'], agents: ['iris'] };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });

/** Chat on the direct message with Iris; `agent()` answers each directory read, `posts` records writes. */
async function irisChat({ agent = () => iris, role = 'Member', messages = [] } = {}, run) {
  let stream, stopLive, cursor = 1, directoryReads = 0;
  const posts = [];
  const original = { EventSource: globalThis.EventSource, IntersectionObserver: globalThis.IntersectionObserver };
  globalThis.EventSource = class { constructor() { this.listeners = new Map(); stream = this; } addEventListener(name, fn) { this.listeners.set(name, fn); } close() {} };
  globalThis.IntersectionObserver = class { observe() {} unobserve() {} disconnect() {} };
  try {
    await reactFixture(async fixture => {
      try {
        await (await import('../src/workspace-context.ts')).enterWorkspace();
        const { startLive } = await import('../src/react/chat/live.ts');
        const { WorkspaceChat } = await import('../src/react/chat/workspace-chat.tsx');
        await fixture.update(() => { stopLive = startLive(); });
        await fixture.render(WorkspaceChat, { href: at('dm-iris') });
        const emit = events => fixture.update(() => stream.listeners.get('changes')({ data: JSON.stringify({ cursor: ++cursor, events }) }));
        const until = async condition => { for (let i = 0; i < 60 && !condition(); i++) await fixture.update(); assert.ok(condition(), `Expected UI did not appear: ${document.body.textContent}`); };
        await run({ ...fixture, emit, until, posts, reads: () => directoryReads });
      } finally { await fixture.update(() => { stopLive?.(); stopLive = undefined; }); }
    }, { url: at('dm-iris'), fetch: async (path, options = {}) => {
      path = new URL(path, 'http://localhost').pathname;
      if (options.method === 'POST') { posts.push({ path, body: JSON.parse(options.body) }); return response({ paused: false, revision: 2 }); }
      if (path.endsWith('/session')) return response({ user: { id: 'dev', email: 'dev@example.test' }, workspaces: [{ id: w, name: 'Aster', role }] });
      if (path.endsWith('/chat-directory')) { directoryReads++; return response({ role, users: [{ id: 'dev', name: 'Dev Singh' }], agents: [agent()] }); }
      if (path.endsWith('/message-capabilities')) return response({ live: true });
      if (path.endsWith('/chats')) return response({ chats: [dm] });
      if (path.endsWith('/messages')) return response({ agents: ['iris'], messages });
      return response({});
    } });
  } finally { Object.assign(globalThis, original); }
}
const line = () => document.querySelector('.chat-pane-header .chat-header-meta')?.textContent;
const headerDot = () => document.querySelector('.chat-pane-header .agent-dot');
const listDot = () => document.querySelector('.chat-directory a.chat-contact .agent-dot');
const button = label => [...document.querySelectorAll('button')].find(node => node.textContent === label || node.getAttribute('aria-label') === label);

test('a ready agent shows a green dot in the list and header, its role, and the message box', async () => {
  await irisChat({}, async ({ until }) => {
    await until(() => line());
    assert.equal(line(), 'Ready · Answers customer questions');
    assert.ok(headerDot().classList.contains('agent-dot-ready'));
    assert.ok(listDot().classList.contains('agent-dot-ready'));
    assert.ok(document.querySelector('.message-composer textarea'));
    assert.equal(document.querySelector('.chat-paused-bar'), null);
    // The name opens the profile; there is no separate Profile button, and a member has no settings link.
    assert.equal(button('Profile'), undefined);
    assert.ok(button('Iris, profile'));
    assert.equal(document.querySelector('a[aria-label="Iris settings"]'), null);
  });
});

test('a paused agent says who can resume it, and its message box gives way to a quiet bar', async () => {
  await irisChat({ agent: () => ({ ...iris, paused: true }) }, async ({ until }) => {
    await until(() => line());
    assert.equal(line(), 'Paused · Ananya Rao can resume it');
    assert.ok(headerDot().classList.contains('agent-dot-paused'));
    assert.ok(listDot().classList.contains('agent-dot-paused'));
    assert.equal(document.querySelector('.message-composer textarea'), null, 'nothing to write that would be refused');
    assert.match(document.querySelector('.chat-paused-bar').textContent, /Iris is paused and takes no new messages/);
    assert.equal(button('Resume Iris'), undefined, 'a member cannot resume it');
  });
});

test('its owner resumes it from the bar once, and an agents change brings the message box back', async () => {
  let paused = true;
  await irisChat({ agent: () => ({ ...iris, yours: true, paused }) }, async ({ update, until, emit, posts }) => {
    await until(() => button('Resume Iris'));
    assert.equal(line(), 'Paused · You can resume it');
    assert.ok(document.querySelector('a[aria-label="Iris settings"]'), 'its owner reaches its settings from the header');
    await update(() => { button('Resume Iris').click(); button('Resume Iris').click(); });
    assert.equal(posts.length, 1, 'a double click resumes once');
    assert.deepEqual([posts[0].path.endsWith('/agent-profiles/iris/pause'), posts[0].body], [true, { paused: false }]);
    paused = false;
    await emit([{ kind: 'agents' }]);
    await until(() => document.querySelector('.message-composer textarea'));
    assert.equal(line(), 'Ready · Answers customer questions');
  });
});

test('all agents paused by an admin, and an agent whose owner left, read as paused with the reason', async () => {
  await irisChat({ agent: () => ({ ...iris, all_paused: true }), role: 'Admin' }, async ({ until }) => {
    await until(() => line());
    assert.equal(line(), 'Paused · An admin paused all agents');
    assert.match(document.querySelector('.chat-paused-bar').textContent, /All agents are paused, so Iris takes no new messages/);
    assert.equal(button('Resume Iris'), undefined, 'all agents resume together in Activity or Settings');
  });
  await irisChat({ agent: () => ({ ...iris, needs_owner: true }) }, async ({ until }) => {
    await until(() => line());
    assert.equal(line(), 'Paused · Its owner left · an admin can give it a new one');
  });
});

test('while it works on your request here the dot turns to working and the line says so', async () => {
  const reply = { id: 'r1', chat: 'dm-iris', author: 'iris', name: 'Iris', kind: 'agent', content: '', state: 'queued', at: 2, sequence: 2, progress: { state: 'responding', text: '' } };
  await irisChat({ messages: [{ id: 'q1', chat: 'dm-iris', author: 'dev', name: 'Dev Singh', kind: 'user', content: 'Refund for order 12?', state: 'sent', at: 1, sequence: 1 }, reply] }, async ({ until }) => {
    await until(() => line() === 'Working on your request…');
    assert.ok(headerDot().classList.contains('agent-dot-working'));
  });
  await irisChat({ messages: [{ ...{ id: 'r1', chat: 'dm-iris', author: 'iris', name: 'Iris', kind: 'agent', content: '', state: 'queued', at: 2, sequence: 2 }, progress: { state: 'approval', text: '' } }] }, async ({ until }) => {
    await until(() => line() === 'Waiting for approval');
  });
});
