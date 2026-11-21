import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

// One profile layout for people and agents (profile-card.tsx, 2026-11-20): who,
// then round actions that depend on where you are and what you may do, then facts.
const w = '11111111-1111-4111-8111-111111111111', base = `http://localhost/w/${w}/?view=chat`;
const people = [
  { id: 'priya', name: 'Priya Shah', title: 'Founder', role: 'Admin', email: 'priya@example.test' },
  { id: 'meera', name: 'Meera Iyer', title: 'Design lead', role: 'Member', email: 'meera@example.test' },
];
const iris = { id: 'iris', name: 'Iris', role: 'Answers customer questions', audience: 'everyone', manager: 'Meera Iyer' };
const dmIris = { id: 'dm-iris', name: 'Iris', kind: 'direct', description: '', users: ['meera'], agents: ['iris'] };
const big = { id: 'big', name: 'big', kind: 'channel', visibility: 'public', description: '', users: [], agents: [], can_archive: true, managers: ['priya'] };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });

function api({ user = 'meera', role = 'Member', agents = [iris], users = people, chats = [dmIris], posts = [] } = {}) {
  return async (path, options = {}) => {
    path = new URL(path, 'http://localhost').pathname.replace(`/api/identity/workspaces/${w}`, '');
    if (options.method === 'POST') { posts.push({ path, body: JSON.parse(options.body) }); return response({ paused: false, revision: 2 }); }
    if (path.endsWith('/session')) return response({ user: { id: user, email: `${user}@example.test` }, workspaces: [{ id: w, name: 'Lotus', role }] });
    if (path.endsWith('/chat-directory')) return response({ role, users, agents });
    if (path.endsWith('/chats')) return response({ chats, preferences: { available: true, revision: 0, pins: [] } });
    if (path.includes('/messages')) return response({ messages: [], agents: [] });
    return response({});
  };
}
async function open(render, conversation) {
  await (await import('../src/workspace-context.ts')).enterWorkspace();
  const { WorkspaceChat } = await import('../src/react/chat/workspace-chat.tsx');
  await render(WorkspaceChat, { href: `${base}&conversation=${conversation}` });
}
const panel = () => document.querySelector('aside.chat-side-panel');
const inPanel = label => [...(panel()?.querySelectorAll('button, a') ?? [])].find(node => node.textContent === label || node.getAttribute('aria-label') === label);
const headerName = () => document.querySelector('.chat-pane-header .chat-header-name');

test('a paused agent’s owner resumes it from its profile, once, and reaches its settings', async () => {
  const posts = [];
  await reactFixture(async ({ render, click, update }) => {
    await open(render, 'dm-iris');
    await click(headerName());
    assert.equal(panel().getAttribute('aria-label'), 'Iris profile');
    assert.match(panel().querySelector('.profile-card-presence').textContent, /Paused · You can resume it/);
    assert.ok(panel().querySelector('.profile-card-face .agent-dot-paused'), 'its face shows the paused dot');
    const actions = [...panel().querySelectorAll('.panel-actions > *')].map(node => node.getAttribute('aria-label') ?? node.textContent);
    assert.deepEqual(actions.slice(0, 1), ['Resume Iris'], 'Resume comes first');
    assert.ok(inPanel('Iris settings'), 'its owner reaches its settings');
    assert.equal(inPanel('Message'), undefined, 'already in your direct message with it');
    const resume = inPanel('Resume Iris');
    await update(() => { resume.click(); resume.click(); });
    assert.deepEqual(posts, [{ path: '/agent-profiles/iris/pause', body: { paused: false } }]);
  }, { url: `${base}&conversation=dm-iris`, fetch: api({ agents: [{ ...iris, paused: true, yours: true, manager: 'Meera Iyer' }], posts }) });
});

test('a member sees an agent’s owner, who opens their own profile, and no Resume or Settings', async () => {
  await reactFixture(async ({ render, click }) => {
    await open(render, 'dm-iris');
    await click(headerName());
    assert.equal(inPanel('Resume Iris'), undefined);
    assert.equal(inPanel('Iris settings'), undefined);
    assert.match(panel().textContent, /Owner: .*Priya Shah/);
    await click(inPanel('View profile of Priya Shah'));
    assert.equal(panel().getAttribute('aria-label'), 'Priya Shah profile');
    assert.ok(inPanel('Message'), 'from another conversation you can message them');
  }, { url: `${base}&conversation=dm-iris`, fetch: api({ user: 'meera', agents: [{ ...iris, manager: 'Priya Shah' }] }) });
});

test('your own profile offers Edit profile, not Message, and an email copies in one tap', async () => {
  let copied;
  await reactFixture(async ({ render, click, dom }) => {
    // The page reads the global navigator, which Node provides; give it a clipboard for this test.
    const before = Object.getOwnPropertyDescriptor(globalThis.navigator, 'clipboard');
    Object.defineProperty(globalThis.navigator, 'clipboard', { configurable: true, value: { writeText: async text => { copied = text; } } });
    dom.window.addEventListener('unload', () => { if (before) Object.defineProperty(globalThis.navigator, 'clipboard', before); else delete globalThis.navigator.clipboard; });
    await open(render, 'dm-iris');
    await click(document.querySelector('.chat-people-button'));
    await click([...document.querySelectorAll('button')].find(node => node.getAttribute('aria-label')?.startsWith('View profile of Meera Iyer')));
    assert.equal(panel().getAttribute('aria-label'), 'Meera Iyer profile');
    assert.equal(inPanel('Message'), undefined, 'not to yourself');
    assert.equal(inPanel('Edit profile').getAttribute('href'), `/w/${w}/?view=settings&section=profile`);
    assert.match(panel().textContent, /Your channels: No channels yet/);
    await click(inPanel('Copy meera@example.test'));
    assert.equal(copied, 'meera@example.test');
    assert.match(panel().textContent, /Copied/);
  }, { url: `${base}&conversation=dm-iris`, fetch: api() });
});

test('a channel with more than eight members gets a search box; a short one does not', async () => {
  const many = Array.from({ length: 9 }, (_, n) => ({ id: `p${n}`, name: `Person ${n}`, role: 'Member' }));
  await reactFixture(async ({ render, click, change }) => {
    await open(render, 'big');
    await click(headerName());
    const search = panel().querySelector('input[type=search]');
    assert.ok(search, 'nine members: a search box');
    await change(search, 'Person 7');
    assert.deepEqual([...panel().querySelectorAll('.chat-details-members .chat-name-text')].map(node => node.textContent), ['Person 7']);
  }, { url: `${base}&conversation=big`, fetch: api({ user: 'p0', users: many, agents: [], chats: [{ ...big, users: many.map(p => p.id) }] }) });
});
