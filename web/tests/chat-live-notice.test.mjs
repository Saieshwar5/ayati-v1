import { test, mock } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';
import { outboxStorage } from './outbox-storage-fixture.mjs';
const w = '11111111-1111-4111-8111-111111111111';
const settle = async () => { for (let i = 0; i < 4; i++) await new Promise(resolve => setImmediate(resolve)); };
const message = id => ({ kind: 'message', chat: 'c1', message: { id, chat: 'c1', author: 'meera', kind: 'user', content: 'Hi', state: 'sent', at: 1 } });

async function withStream(run) {
  const previous = { indexedDB: globalThis.indexedDB, EventSource: globalThis.EventSource };
  const streams = [];
  globalThis.indexedDB = outboxStorage();
  globalThis.EventSource = class {
    constructor(url) { this.url = url; this.listeners = new Map(); this.closed = false; streams.push(this); }
    addEventListener(name, fn) { this.listeners.set(name, fn); }
    close() { this.closed = true; }
    emit(name, data = '{}') { this.listeners.get(name)?.({ data: typeof data === 'string' ? data : JSON.stringify(data) }); }
  };
  try {
    await reactFixture(async fixture => {
      const context = await import('../src/workspace-context.ts');
      assert.equal(await context.enterWorkspace(), true);
      const live = await import('../src/react/chat/live.ts');
      const notices = await import('../src/react/chat/live-notice.ts');
      mock.timers.enable({ apis: ['setTimeout', 'setInterval', 'Date'], now: 0 });
      Object.assign(fixture.dom.window, { setTimeout, setInterval, clearTimeout, clearInterval });
      const stop = live.startLive();
      try {
        await settle();
        await run({ ...fixture, live, notices, streams, tick: async ms => { mock.timers.tick(ms); await settle(); } });
      } finally { stop?.(); mock.timers.reset(); notices.clearNotice(notices.liveNotice()?.kind ?? 'back'); }
    }, { url: `http://localhost/w/${w}/`, fetch: async path => {
      if (path.endsWith('/session')) return { ok: true, status: 200, json: async () => ({ user: { id: 'owner', email: 'owner@example.test' }, workspaces: [{ id: w, name: 'Test', role: 'Admin' }] }) };
      if (path.endsWith('/message-capabilities')) return { ok: true, status: 200, json: async () => ({ live: true }) };
      return { ok: true, status: 200, json: async () => ({ ok: true }) };
    } });
  } finally { Object.assign(globalThis, previous); }
}

test('after a drop the stream says how many messages it caught up, once every page is in', async () => {
  await withStream(async ({ live, notices, streams, tick }) => {
    streams[0].emit('changes', { cursor: 1, events: [] });
    assert.equal(notices.liveNotice(), undefined, 'a first connect has no gap to report');
    streams[0].emit('changes', { cursor: 2, events: [message('m0')] });
    assert.equal(notices.liveNotice(), undefined, 'live messages while connected are not a catch-up');

    streams[0].onerror();
    assert.equal(live.connectionProblem(live.liveStatus()), 'reconnecting');
    await tick(1600);
    assert.equal(streams.length, 2);
    // Two pages: the notice waits for the last one, and an edit of the same message counts once.
    streams[1].emit('changes', { cursor: 4, more: true, events: [message('m1'), message('m2')] });
    assert.equal(notices.liveNotice(), undefined);
    streams[1].emit('changes', { cursor: 5, events: [message('m2'), { kind: 'access' }] });
    assert.deepEqual(notices.liveNotice(), { kind: 'back', messages: 2 });
    assert.equal(live.liveStatus(), 'Connected');

    // Further live batches leave the notice alone and start no new count.
    notices.clearNotice('back');
    streams[1].emit('changes', { cursor: 6, events: [message('m3')] });
    assert.equal(notices.liveNotice(), undefined);
  });
});

test('a gap longer than the server keeps reloads the screens and says so; a first snapshot does not', async () => {
  await withStream(async ({ notices, streams, tick }) => {
    streams[0].emit('changes', { cursor: 10, reset: true, events: [] });
    assert.equal(notices.liveNotice(), undefined, 'no cursor yet: an ordinary first load');
    streams[0].onerror();
    await tick(1600);
    assert.match(streams[1].url, /after=10/);
    streams[1].emit('changes', { cursor: 90, reset: true, events: [] });
    assert.deepEqual(notices.liveNotice(), { kind: 'reloaded' });
  });
});

test('a stream from a newer build asks this tab to reload; the same or a development build does not', async () => {
  await withStream(async ({ dom, notices, streams }) => {
    streams[0].emit('version', { client: '/assets/main-NEW.js' });
    assert.equal(notices.liveNotice(), undefined, 'the development server has no built entry to compare');
    const script = dom.window.document.createElement('script');
    script.type = 'module'; script.setAttribute('src', '/assets/main-OLD.js');
    dom.window.document.head.append(script);
    streams[0].emit('version', { client: '/assets/main-OLD.js' });
    streams[0].emit('version', { client: null });
    streams[0].emit('version', 'not json');
    assert.equal(notices.liveNotice(), undefined);
    streams[0].emit('version', { client: '/assets/main-NEW.js' });
    assert.deepEqual(notices.liveNotice(), { kind: 'updated' });
    // Catching up afterwards never hides the reload prompt.
    notices.showNotice({ kind: 'back', messages: 3 });
    assert.deepEqual(notices.liveNotice(), { kind: 'updated' });
  });
});

test('the banner shows each notice, fades "Back online" and keeps the reload prompt', async () => {
  await reactFixture(async ({ dom, render, button, click, update }) => {
    const { ConnectionBanner, BACK_ONLINE_MS } = await import('../src/react/chat/connection-banner.tsx');
    const notices = await import('../src/react/chat/live-notice.ts');
    mock.timers.enable({ apis: ['setTimeout'] });
    Object.assign(dom.window, { setTimeout, clearTimeout });
    try {
      await render(ConnectionBanner, { status: 'Connected' });
      assert.equal(document.querySelector('.chat-connection-banner'), null);
      await update(() => notices.showNotice({ kind: 'back', messages: 1 }));
      assert.equal(document.querySelector('[role=status]').textContent, 'Back online. Caught up on 1 message.');
      await update(() => mock.timers.tick(BACK_ONLINE_MS));
      assert.equal(document.querySelector('.chat-connection-banner'), null, '"Back online" fades');

      await update(() => notices.showNotice({ kind: 'reloaded' }));
      assert.match(document.querySelector('[role=status]').textContent, /You were away for a while\. Showing the latest messages\./);
      await click(button('Dismiss'));
      assert.equal(document.querySelector('.chat-connection-banner'), null);

      await update(() => notices.showNotice({ kind: 'updated' }));
      await update(() => mock.timers.tick(BACK_ONLINE_MS * 2));
      assert.match(document.querySelector('[role=status]').textContent, /Cuckoo was updated\. Reload to keep chatting\. Your drafts are kept\./);
      assert.ok(button('Reload'));

      // While the connection is down, that comes first.
      await render(ConnectionBanner, { status: 'Reconnecting… Messages remain on this device until confirmed.' });
      assert.match(document.querySelector('[role=status]').textContent, /Reconnecting… You can keep writing/);
    } finally { mock.timers.reset(); notices.clearNotice('updated'); }
  });
});
