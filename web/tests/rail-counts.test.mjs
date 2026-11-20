import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { act } from 'react';
import { outboxStorage } from './outbox-storage-fixture.mjs';
import { reactFixture } from './react-fixture.mjs';

const w = '11111111-1111-4111-8111-111111111111', url = `http://localhost/w/${w}/?view=home`;
const session = { user: { id: 'meera', email: 'meera@example.test' }, workspaces: [{ id: w, name: 'Studio', role: 'Member' }] };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const named = node => { const copy = node.cloneNode(true); copy.querySelectorAll('[aria-hidden=true]').forEach(part => part.remove()); return copy.textContent.trim(); };
const item = id => ({ kind: 'approval', id, version: '1', title: `Approve ${id}`, status: 'pending', at: 1, agent: 'a', agent_name: 'Stock assistant', chat: 'orders', thread: null, reply: `r-${id}`, conversation_name: 'orders', archived: false, read: false, reviewed: false });

test('the tab title says how much is waiting, capped at 99+', async () => {
  await reactFixture(async () => {
    const { documentTitle } = await import('../src/react/document-title.tsx');
    assert.equal(documentTitle('home', 0), 'Home · Cuckoo');
    assert.equal(documentTitle('chat', 3), '(3) Chat · Cuckoo');
    assert.equal(documentTitle('activity', 26, true), '(26+) Activity · Cuckoo');
    assert.equal(documentTitle('settings', 250), '(99+) Settings · Cuckoo');
  }, { url });
});

test('the rail counts what needs you on every page, and the tab title adds unread chat', async () => {
  let stream, cursor = 1, attention = { available: true, items: [item('a1'), item('a2')], next_before: null }, denied = false;
  const reads = [];
  const original = Object.fromEntries(['indexedDB', 'Event', 'EventSource', 'Option', 'FormData', 'sessionStorage', 'matchMedia', 'requestAnimationFrame', 'cancelAnimationFrame'].map(key => [key, globalThis[key]]));
  await reactFixture(async ({ dom, render, click, update }) => {
    dom.window.setInterval = () => 0; dom.window.clearInterval = () => {};
    Object.assign(globalThis, { indexedDB: outboxStorage(), Event: dom.window.Event, Option: dom.window.Option, FormData: dom.window.FormData, sessionStorage: dom.window.sessionStorage,
      requestAnimationFrame: handler => { handler(); return 0; }, cancelAnimationFrame: () => {}, matchMedia: () => ({ matches: false, addEventListener() {}, removeEventListener() {} }),
      EventSource: class { constructor(address) { this.listeners = new Map(); if (address.includes('message-events')) stream = this; } addEventListener(name, fn) { this.listeners.set(name, fn); } close() {} } });
    try {
      await (await import('../src/workspace-context.ts')).enterWorkspace();
      const { WorkspaceApplication } = await import('../src/react/workspace-application.tsx');
      await render(WorkspaceApplication);
      const until = async condition => { for (let i = 0; i < 80 && !condition(); i++) await update(); assert.ok(condition(), `Expected UI did not appear: ${document.body.textContent}`); };
      const activity = () => document.querySelector('.workspace-rail [title=Activity]');
      const badge = () => activity().querySelector('.workspace-nav-unread');
      const emit = events => update(() => stream.listeners.get('changes')({ data: JSON.stringify({ cursor: ++cursor, events }) }));
      await until(() => document.querySelector('.home') && stream && badge());

      assert.equal(badge().textContent, '2');
      assert.equal(named(activity()), 'Activity, 2 need you');
      await until(() => document.title === '(3) Home · Cuckoo');

      // Answered elsewhere: an access change reads it again at once.
      attention = { ...attention, items: [item('a1')] };
      await emit([{ kind: 'access' }]);
      await until(() => badge()?.textContent === '1');
      assert.equal(named(activity()), 'Activity, 1 needs you');

      // On another page the rail still counts, and so does the title.
      await click(document.querySelector('.workspace-rail [title=Settings]'));
      await until(() => document.querySelector('.settings-page'));
      await until(() => document.title === '(2) Settings · Cuckoo');
      attention = { available: true, items: Array.from({ length: 25 }, (_, n) => item(`b${n}`)), next_before: 'cursor' };
      await emit([{ kind: 'access' }]);
      await until(() => badge()?.textContent === '25+');
      assert.equal(named(activity()), 'Activity, 25 or more need you');
      await until(() => document.title === '(26+) Settings · Cuckoo');

      // Losing access clears the count.
      denied = true;
      await emit([{ kind: 'access' }]);
      await until(() => !badge());
      assert.equal(named(activity()), 'Activity');
      await until(() => document.title === '(1) Settings · Cuckoo');
      assert.ok(reads.every(path => path.endsWith('view=needs_you')), 'only the first page of Needs me is read');
    } finally {
      await act(async () => { await Promise.resolve(); });
      Object.assign(globalThis, original);
    }
  }, { url, fetch: async path => {
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/message-capabilities')) return response({ live: true });
    if (path.endsWith('/chat-directory')) return response({ role: 'Member', users: [], agents: [] });
    if (path.endsWith('/unread')) return response({ unread: [{ chat: 'orders', count: 1, mentions: 0, muted: false }] });
    if (path.includes('/attention?')) { reads.push(path); return denied ? response({ error: 'No access' }, 403) : response(attention); }
    if (path.endsWith('/home-panels')) return response({ available: false, panels: [] });
    if (path.endsWith('/work')) return response({ work: [] });
    if (path.endsWith('/preferences')) return response({ desktop_alerts: true, channel_level: 'mentions', approval_alerts: true, show_online: true });
    throw new Error(`Unexpected request: ${path}`);
  } });
});
