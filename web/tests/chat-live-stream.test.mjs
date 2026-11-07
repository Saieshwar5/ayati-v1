import { test, mock } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';
import { outboxStorage } from './outbox-storage-fixture.mjs';
const w = '11111111-1111-4111-8111-111111111111';
const settle = async () => { for (let i = 0; i < 4; i++) await new Promise(resolve => setImmediate(resolve)); };

test('an idle message stream held by alive events stays open; a silent one reconnects', async () => {
  const previous = { indexedDB: globalThis.indexedDB, EventSource: globalThis.EventSource };
  const streams = [];
  globalThis.indexedDB = outboxStorage();
  globalThis.EventSource = class {
    constructor(url) { this.url = url; this.listeners = new Map(); this.closed = false; streams.push(this); }
    addEventListener(name, fn) { this.listeners.set(name, fn); }
    close() { this.closed = true; }
    emit(name, data = '{}') { this.listeners.get(name)?.({ data }); }
  };
  try {
    await reactFixture(async ({ dom }) => {
      const context = await import('../src/workspace-context.ts');
      assert.equal(await context.enterWorkspace(), true);
      const live = await import('../src/react/chat/live.ts');
      mock.timers.enable({ apis: ['setTimeout', 'setInterval', 'Date'], now: 0 });
      // live.ts schedules through window; route those timers to the mocked clock.
      Object.assign(dom.window, { setTimeout, setInterval, clearTimeout, clearInterval });
      let changes = 0, statusUpdates = 0;
      const unsubscribeChanges = live.subscribeChanges(() => { changes++; });
      const unsubscribeStatus = live.subscribeStatus(() => { statusUpdates++; });
      const stop = live.startLive();
      const advance = async seconds => { for (let i = 0; i < seconds; i++) { mock.timers.tick(1000); await settle(); } };
      try {
        await settle();
        assert.equal(streams.length, 1);
        streams[0].emit('changes', JSON.stringify({ cursor: 1, events: [] }));
        assert.equal(live.liveStatus(), 'Connected');
        const updatesWhenConnected = statusUpdates;

        // Idle: the server sends nothing but an alive event every 15 seconds.
        for (let second = 1; second <= 60; second++) {
          await advance(1);
          if (second % 15 === 0) streams[0].emit('alive');
          if ([11, 30, 60].includes(second)) {
            assert.equal(streams.length, 1, `no reconnect after ${second} idle seconds`);
            assert.equal(streams[0].closed, false);
            assert.equal(live.liveStatus(), 'Connected');
          }
        }
        assert.equal(changes, 0, 'alive events publish no changes');
        assert.equal(statusUpdates, updatesWhenConnected, 'alive events leave the status alone');

        // Silent: no events at all after the last alive event at 60 seconds.
        await advance(44);
        assert.equal(streams.length, 1, 'still open within the silence limit');
        await advance(2);
        assert.equal(streams.length, 2, 'a stream silent past the limit is replaced');
        assert.equal(streams[0].closed, true);
        assert.equal(live.liveStatus(), 'Connecting…');

        // A new stream sends its first batch at once; one that stays silent while
        // connecting has hung and is retried after 15 seconds, not 45.
        await advance(14);
        assert.equal(streams.length, 2, 'a connecting stream gets 15 seconds');
        await advance(2);
        assert.equal(streams.length, 3, 'a silent connect is retried sooner');
      } finally {
        stop?.(); unsubscribeChanges(); unsubscribeStatus(); mock.timers.reset();
      }
    }, { url: `http://localhost/w/${w}/`, fetch: async path => {
      if (path.endsWith('/session')) return { ok: true, status: 200, json: async () => ({ user: { id: 'owner', email: 'owner@example.test' }, workspaces: [{ id: w, name: 'Test', role: 'Admin' }] }) };
      if (path.endsWith('/message-capabilities')) return { ok: true, status: 200, json: async () => ({ live: true }) };
      return { ok: true, status: 200, json: async () => ({ ok: true }) };
    } });
  } finally {
    Object.assign(globalThis, previous);
  }
});
