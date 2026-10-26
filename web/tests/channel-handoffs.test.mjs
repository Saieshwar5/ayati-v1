import { registerHooks } from 'node:module';
registerHooks({
  load(url, c, next) {
    return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, c);
  },
});
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';
const w = '11111111-1111-4111-8111-111111111111',
  url = `http://localhost/w/${w}/?view=chat`;
const session = {
  user: { id: 'owner', email: 'owner@example.test' },
  workspaces: [{ id: w, name: 'Store', role: 'Owner' }],
};
const response = (v, status = 200) => ({ ok: status < 400, status, json: async () => v });
const handoff = {
  id: 'handoff',
  parent: 'parent',
  root: 'parent',
  child: 'child',
  request_message: 'request',
  sender_name: 'Nova',
  target_name: 'Atlas',
  principal_name: 'Ananya',
  chat: 'channel',
  thread: 'parent',
  task: 'PRIVATE_HANDOFF_TASK',
  state: 'pending',
  detail: '',
  task_state: 'queued',
  job_state: 'waiting',
  phase: 'approval',
  can_stop: true,
};
async function setup() {
  const c = await import('../src/workspace-context.ts');
  await c.enterWorkspace();
  return import('../src/react/chat/handoffs.tsx');
}

test('handoff reading shows original authority and exact navigation without starting a task', async () => {
  const writes = [];
  await reactFixture(
    async ({ render }) => {
      const { HandoffCard } = await setup();
      await render(HandoffCard, { handoff: { ...handoff, can_stop: false } });
      assert.match(document.body.textContent, /Nova → Atlas/);
      assert.match(document.body.textContent, /Waiting for approval/);
      assert.match(document.body.textContent, /Requested by Ananya/);
      assert.match(document.body.textContent, /Nova's limits/);
      const links = [...document.querySelectorAll('a')];
      assert.match(links[0].href, /message=child&thread=parent/);
      assert.match(links[1].href, /message=parent/);
      assert.equal(document.querySelector('button'), null);
      assert.deepEqual(writes, []);
    },
    {
      url,
      fetch: async (path, init = {}) => {
        if (init.method === 'POST') writes.push(path);
        return response(session);
      },
    },
  );
});

test('Stop helper is synchronously guarded and failed saves retain the task instead of claiming completion', async () => {
  const writes = [];
  let finish;
  await reactFixture(
    async ({ render, button, update }) => {
      const { HandoffCard } = await setup();
      await render(HandoffCard, { handoff });
      await update(() => {
        button('Stop helper task').click();
        button('Stop helper task').click();
      });
      assert.equal(writes.length, 1);
      assert.ok(writes[0].endsWith('/messages/child/cancel'));
      assert.match(document.body.textContent, /Waiting for approval/);
      await update(() => finish(response({ error: 'Stop was not acknowledged' }, 503)));
      assert.match(document.body.textContent, /Stop was not acknowledged/);
      assert.match(document.body.textContent, /Waiting for approval/);
      assert.equal(button('Stop helper task').disabled, false);
    },
    {
      url,
      fetch: async (path, init = {}) =>
        init.method === 'POST'
          ? (writes.push(path),
            new Promise((resolve) => {
              finish = resolve;
            }))
          : response(session),
    },
  );
});

test('a source event clears cached handoff instructions immediately and preserves an unrelated chat draft', async () => {
  let stream,
    stopLive,
    revoked = false;
  const deferred = [],
    previous = globalThis.EventSource;
  globalThis.EventSource = class {
    constructor() {
      this.listeners = new Map();
      stream = this;
    }
    addEventListener(k, fn) {
      this.listeners.set(k, fn);
    }
    close() {}
  };
  try {
    await reactFixture(
      async ({ render, change, update }) => {
        try {
          await setup();
          const { startLive } = await import('../src/react/chat/live.ts');
          const { ConversationPane } = await import('../src/react/chat/conversation.tsx');
          await update(() => {
            stopLive = startLive();
          });
          await render(ConversationPane, {
            chat: { id: 'channel', name: 'Operations', kind: 'channel', users: ['owner'], agents: ['a', 'b'] },
            directory: {
              role: 'Owner',
              users: [{ id: 'owner', name: 'Ananya' }],
              agents: [
                { id: 'a', name: 'Nova' },
                { id: 'b', name: 'Atlas' },
              ],
            },
            back: () => {},
          });
          assert.match(document.body.textContent, /PRIVATE_HANDOFF_TASK/);
          const composer = document.querySelector('[aria-label=Message]');
          await change(composer, 'KEEP_UNRELATED_DRAFT');
          revoked = true;
          await update(() =>
            stream.listeners.get('changes')({
              data: JSON.stringify({
                cursor: 42,
                events: [{ kind: 'event_access', source: 'source-scope', chat: 'channel' }],
              }),
            }),
          );
          assert.doesNotMatch(document.body.textContent, /PRIVATE_HANDOFF_TASK/);
          assert.equal(document.querySelector('[aria-label=Message]').value, 'KEEP_UNRELATED_DRAFT');
          await update(() => deferred.forEach((resolve) => resolve(response({ error: 'Source denied' }, 403))));
          assert.doesNotMatch(document.body.textContent, /PRIVATE_HANDOFF_TASK/);
        } finally {
          await update(() => {
            stopLive?.();
            stopLive = undefined;
          });
        }
      },
      {
        url,
        fetch: async (path) => {
          if (path.endsWith('/session')) return response(session);
          if (path.endsWith('/message-capabilities')) return response({ live: true });
          if (path.includes('/messages?'))
            return revoked
              ? new Promise((resolve) => deferred.push(resolve))
              : response({
                  messages: [
                    {
                      id: 'parent',
                      chat: 'channel',
                      author: 'a',
                      kind: 'agent',
                      content: 'Waiting on helper.',
                      state: 'queued',
                      progress: { state: 'handoff', text: '' },
                      source_scope: 'source-scope',
                      revision: 1,
                      sequence: 1,
                      at: 1,
                      coordination: { incoming: null, outgoing: [handoff] },
                    },
                  ],
                  agents: ['a', 'b'],
                });
          return response({});
        },
      },
    );
  } finally {
    globalThis.EventSource = previous;
  }
});
