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
const session = { user: { id: 'owner', email: 'owner@example.com' }, workspaces: [{ id: workspace, name: 'Store', role: 'Admin' }] };
const inventory = { id: 'inventory', name: 'Inventory', kind: 'channel', description: 'Stock', users: ['owner', 'member'], agents: [] };
const response = data => ({ ok: true, status: 200, json: async () => data });

/** The whole workspace app, opened on bare Chat; `chats()` answers each list read. */
async function workspaceApp(chats, run) {
  await reactFixture(async fixture => {
    const { dom, update } = fixture;
    const keys = ['indexedDB', 'Event', 'EventSource', 'Option', 'FormData', 'sessionStorage', 'matchMedia', 'requestAnimationFrame', 'cancelAnimationFrame', 'IntersectionObserver'];
    const original = Object.fromEntries(keys.map(key => [key, globalThis[key]]));
    dom.window.setInterval = () => 0; dom.window.clearInterval = () => {};
    Object.assign(globalThis, { indexedDB: outboxStorage(), Event: dom.window.Event, Option: dom.window.Option, FormData: dom.window.FormData,
      sessionStorage: dom.window.sessionStorage, requestAnimationFrame: handler => { handler(); return 0; }, cancelAnimationFrame: () => {},
      matchMedia: () => ({ matches: false, addEventListener() {}, removeEventListener() {} }),
      IntersectionObserver: class { observe() {} unobserve() {} disconnect() {} },
      EventSource: class { addEventListener() {} close() {} } });
    try {
      await (await import('../src/workspace-context.ts')).enterWorkspace();
      const { WorkspaceApplication } = await import('../src/react/workspace-application.tsx');
      await fixture.render(WorkspaceApplication);
      const until = async condition => { for (let i = 0; i < 80 && !condition(); i++) await update(); assert.ok(condition(), `Expected UI did not appear: ${document.body.textContent}`); };
      const rail = label => document.querySelector(`.workspace-rail [title=${label}]`);
      await run({ ...fixture, until, rail });
    } finally {
      await act(async () => { await Promise.resolve(); });
      Object.assign(globalThis, original);
    }
  }, { url: `http://localhost${base}?view=chat`, fetch: async path => {
    path = new URL(path, 'http://localhost').pathname;
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/chat-directory')) return response({ users: [{ id: 'member', name: 'Priya' }], agents: [], role: 'Admin' });
    if (path.endsWith('/chats')) return response({ chats: chats() });
    if (path.endsWith('/messages')) return response({ agents: [], messages: [{ id: 'm1', author: 'member', name: 'Priya', kind: 'user', content: 'Check stock', state: 'sent', at: 1, thread: null }] });
    if (path.endsWith('/for-you')) return response({ available: true, new: [], earlier: [], next_before: null });
    if (path.endsWith('/home-panels')) return response({ available: false, panels: [] });
    if (path.endsWith('/suggestions')) return response({ suggestions: [] });
    if (path.endsWith('/message-capabilities')) return response({ live: true });
    if (path.endsWith('/unread')) return response({ unread: [] });
    if (path.includes('/attention')) return response({ available: true, items: [], next_before: null });
    if (path.endsWith('/work')) return response({ work: [] });
    return response({});
  } });
}
const welcome = () => document.querySelector('.chat-welcome h2')?.textContent;
const header = () => document.querySelector('.chat-pane-header h2')?.textContent;

test('coming back to Chat from another page reopens the last conversation, and Back skips the bare Chat step', async () => {
  await workspaceApp(() => [inventory], async ({ dom, click, update, until, rail }) => {
    await until(() => welcome() === 'A place to work together');
    await click([...document.querySelectorAll('.chat-directory a')].find(a => a.textContent.includes('Inventory')));
    await until(() => header() === 'Inventory');
    await click(rail('Home'));
    await until(() => document.querySelector('.home'));
    await click(rail('Chat'));
    await until(() => header() === 'Inventory');
    assert.equal(document.querySelector('.chat-welcome'), null, 'no first-visit welcome');
    assert.equal(dom.window.location.search, '?view=chat&conversation=inventory');
    await update(() => dom.window.history.back());
    await until(() => document.querySelector('.home'));
    // Already in Chat, the rail still offers the plain Chat start (the list on phones).
    await click(rail('Chat'));
    await until(() => header() === 'Inventory');
    await click(rail('Chat'));
    await until(() => welcome() === 'A place to work together');
  });
});

test('Chat → For you is remembered too', async () => {
  await workspaceApp(() => [inventory], async ({ click, until, rail }) => {
    await until(() => document.querySelector('a.chat-for-you'));
    await click(document.querySelector('a.chat-for-you'));
    await until(() => document.querySelector('.for-you-empty'));
    await click(rail('Settings'));
    await until(() => document.querySelector('.settings-page'));
    await click(rail('Chat'));
    await until(() => document.querySelector('.for-you-empty'));
  });
});

test('a remembered conversation that is gone falls back to the welcome and is forgotten', async () => {
  let chats = [inventory];
  await workspaceApp(() => chats, async ({ dom, click, until, rail }) => {
    await until(() => document.querySelector('.chat-contact'));
    await click([...document.querySelectorAll('.chat-directory a')].find(a => a.textContent.includes('Inventory')));
    await until(() => header() === 'Inventory');
    await click(rail('Home'));
    await until(() => document.querySelector('.home'));
    chats = [];
    await click(rail('Chat'));
    await until(() => welcome() === 'A place to work together');
    assert.equal(dom.window.location.search, '?view=chat');
    assert.equal(dom.window.sessionStorage.getItem(`relay.chat.place.owner.${workspace}`), null);
  });
});
