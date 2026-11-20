// Chat's first visit (chat-start.tsx, add-someone.tsx, assistant-start.tsx):
// what someone sees with nobody else here, by role, and where each tile leads.
import { act } from 'react';
import { outboxStorage } from './outbox-storage-fixture.mjs';
import { registerHooks } from 'node:module';
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

registerHooks({ load(url, context, next) {
  if (url.endsWith('.css')) return { format: 'module', source: 'export {};', shortCircuit: true };
  return next(url, context);
} });

const workspace = '11111111-1111-4111-8111-111111111111';
const base = `/w/${workspace}/`;
const response = data => ({ ok: true, status: 200, json: async () => data });
const me = { id: 'owner', name: 'Diya Thomas' };
const ayati = { id: 'mine', name: 'Ayati', role: 'Your assistant', audience: 'only_me', yours: true, assistant: true };
const hr = { id: 'hr', name: 'HR Desk', role: 'Pay and leave', audience: 'everyone' };
const assistantChat = { id: 'dm-mine', name: 'Ayati', kind: 'direct', description: '', users: ['owner'], agents: ['mine'] };

/** The whole workspace app; `requests` records every call with its method and body. */
async function workspaceApp({ query = '?view=chat', role = 'Admin', users = [me], agents = [ayati], chats = [assistantChat], invitations = [], messages = [] }, run) {
  const requests = [];
  await reactFixture(async fixture => {
    const { dom, update } = fixture;
    const keys = ['indexedDB', 'Event', 'EventSource', 'Option', 'FormData', 'sessionStorage', 'matchMedia', 'requestAnimationFrame', 'cancelAnimationFrame', 'IntersectionObserver', 'KeyboardEvent'];
    const original = Object.fromEntries(keys.map(key => [key, globalThis[key]]));
    dom.window.setInterval = () => 0; dom.window.clearInterval = () => {};
    Object.assign(globalThis, { indexedDB: outboxStorage(), Event: dom.window.Event, KeyboardEvent: dom.window.KeyboardEvent, Option: dom.window.Option, FormData: dom.window.FormData,
      sessionStorage: dom.window.sessionStorage, requestAnimationFrame: handler => { handler(); return 0; }, cancelAnimationFrame: () => {},
      matchMedia: () => ({ matches: false, addEventListener() {}, removeEventListener() {} }),
      IntersectionObserver: class { observe() {} unobserve() {} disconnect() {} },
      EventSource: class { addEventListener() {} close() {} } });
    try {
      await (await import('../src/workspace-context.ts')).enterWorkspace();
      const { WorkspaceApplication } = await import('../src/react/workspace-application.tsx');
      await fixture.render(WorkspaceApplication);
      const until = async condition => { for (let i = 0; i < 80 && !condition(); i++) await update(); assert.ok(condition(), `Expected UI did not appear: ${document.body.textContent}`); };
      const escape = node => update(() => node.dispatchEvent(new dom.window.KeyboardEvent('keydown', { key: 'Escape', bubbles: true })));
      await run({ ...fixture, until, escape, requests });
    } finally {
      await act(async () => { await Promise.resolve(); });
      Object.assign(globalThis, original);
    }
  }, { url: `http://localhost${base}${query}`, fetch: async (path, options = {}) => {
    const method = options.method ?? 'GET';
    path = new URL(path, 'http://localhost').pathname;
    requests.push({ path, method, body: options.body ? JSON.parse(options.body) : undefined });
    if (path.endsWith('/session')) return response({ user: { id: 'owner', email: 'diya@example.com' }, workspaces: [{ id: workspace, name: 'Northwind', role }] });
    if (path.endsWith('/chat-directory')) return response({ users, agents, role });
    if (path.endsWith('/chats') && method === 'POST') return response({ id: 'dm-new' });
    if (path.endsWith('/chats')) return response({ chats });
    if (path.endsWith('/people')) return response({ role, members: [], invitations });
    if (path.endsWith('/messages')) return response({ agents: ['mine'], messages });
    if (path.endsWith('/for-you')) return response({ available: true, new: [], earlier: [], next_before: null });
    if (path.endsWith('/agent-profiles')) return response({ role, agents: [], default_model: 'fixture', system_prompt: 'Rules.', providers: [] });
    if (path.endsWith('/agent-draft')) return response({ draft: null });
    if (path.endsWith('/home-panels')) return response({ available: false, panels: [] });
    if (path.endsWith('/unread')) return response({ unread: [] });
    if (path.includes('/attention')) return response({ available: true, items: [], next_before: null });
    return response({});
  } });
}
const start = () => document.querySelector('.chat-start');
const link = text => [...document.querySelectorAll('a')].find(node => node.textContent.includes(text));
const later = Math.floor(Date.now() / 1000) + 86400;

test('an admin alone sees their assistant, Invite people and Create agent, and ＋ offers them instead of a form', async () => {
  await workspaceApp({ invitations: [{ id: 'i1', email: 'meera@example.com', status: 'pending', expires: later }, { id: 'i2', email: 'old@example.com', status: 'pending', expires: 1 }] }, async ({ dom, button, click, escape, until }) => {
    await until(() => start());
    assert.match(start().querySelector('h2').textContent, /Ayati/);
    const invite = start().querySelector(`a[href="${base}?view=settings&section=people"]`);
    await until(() => invite.getAttribute('aria-label') === 'Invite people, 1 invitation waiting');
    assert.equal(start().querySelector('.chat-start-waiting').title, 'meera@example.com', 'an expired invitation is not shown');
    assert.equal(link('Create agent').getAttribute('href'), `${base}?view=agents&new=1`);
    assert.ok(document.querySelector('.chat-directory .chat-invite-row'), 'an empty Direct messages list offers Invite people');

    // Channels ＋ with nobody who could join: the choice, not a form that cannot be saved.
    await click(button('Create channel'));
    const card = document.querySelector('.chat-add-someone');
    assert.equal(card.getAttribute('aria-label'), 'Add someone first');
    assert.equal(document.querySelector('.ui-dialog'), null, 'no Create channel dialog');
    assert.equal(document.activeElement, card.querySelector('a'), 'the first choice has focus');
    await escape(card);
    assert.equal(document.querySelector('.chat-add-someone'), null);
    assert.equal(document.activeElement, button('Create channel'), 'focus goes back to the ＋');

    // Direct messages ＋: Invite comes first, since you came to message a person.
    await click(button('New message'));
    const tiles = [...document.querySelectorAll('.chat-add-someone .chat-start-tile')];
    assert.deepEqual(tiles.map(tile => tile.textContent.replace('+', '')), ['MInvite people', 'Create agent']);
    assert.ok(tiles[0].classList.contains('chat-start-tile-primary'));

    // Ask Ayati opens your assistant's chat.
    await click(button('Ask Ayati'));
    assert.match(dom.window.location.search, /conversation=dm-mine/);
  });
});

test('a member never sees Invite, never reads invitations, and one team agent is enough for a channel', async () => {
  await workspaceApp({ role: 'Member', agents: [ayati, hr] }, async ({ button, click, until, requests }) => {
    await until(() => start());
    assert.equal(document.body.textContent.includes('Invite people'), false);
    assert.ok(link('Create agent'));
    await click(button('Create channel'));
    await until(() => document.querySelector('.ui-dialog'));
    assert.equal(document.querySelector('.chat-add-someone'), null);
    const choices = [...document.querySelectorAll('.chat-choice')].map(choice => choice.textContent);
    assert.deepEqual(choices, ['HR DeskAIPay and leave'], 'your assistant never joins a channel');
    assert.equal(document.activeElement?.placeholder, 'e.g. inventory', 'the cursor starts in the channel name');
    assert.equal(requests.some(request => request.path.endsWith('/people')), false);
  });
});

test('people already here are one click from a direct message', async () => {
  const users = [me, { id: 'priya', name: 'Priya Shah' }, { id: 'arjun', name: 'Arjun Mehta' }];
  await workspaceApp({ role: 'Member', users }, async ({ dom, button, click, until, requests }) => {
    await until(() => start());
    assert.deepEqual([...start().querySelectorAll('.chat-start-people button')].map(node => node.getAttribute('aria-label')),
      ['Message Arjun Mehta', 'Message Priya Shah', 'All people and agents']);
    await click(button('Message Priya Shah'));
    await until(() => /conversation=dm-new/.test(dom.window.location.search));
    const created = requests.filter(request => request.method === 'POST' && request.path.endsWith('/chats'));
    assert.equal(created.length, 1);
    assert.deepEqual([created[0].body.kind, created[0].body.users, created[0].body.agents], ['direct', ['priya'], []]);
  });
});

test('New message starts in its search field', async () => {
  await workspaceApp({ role: 'Member', agents: [ayati, hr] }, async ({ button, click, until }) => {
    await until(() => start());
    await click(button('New message'));
    await until(() => document.querySelector('.ui-dialog'));
    assert.equal(document.activeElement?.type, 'search');
  });
});

test('your empty assistant chat offers examples that fill the box and never send', async () => {
  await workspaceApp({ query: '?view=chat&conversation=dm-mine' }, async ({ button, click, until, requests }) => {
    await until(() => document.querySelector('.chat-assistant-intro'));
    assert.match(document.querySelector('.chat-assistant-intro').textContent, /Only you can talk to Ayati · admins can see its work/);
    assert.equal(document.querySelector('.chat-visibility-note'), null, 'the intro replaces the note');
    await click(button('Draft a message'));
    assert.equal(document.querySelector('.message-composer textarea').value, 'Help me write a message to my team about ');
    assert.equal(document.querySelector('.chat-examples'), null, 'examples step aside once you are writing');
    assert.equal(requests.some(request => request.method === 'POST' && request.path.endsWith('/messages')), false);
  });
});

test('Create agent opens the create steps once, and the link leaves the address', async () => {
  await workspaceApp({ query: '?view=agents&new=1', role: 'Member' }, async ({ dom, until }) => {
    await until(() => document.querySelector('.agent-create'));
    assert.equal(new URLSearchParams(dom.window.location.search).has('new'), false);
  });
});
