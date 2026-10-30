import { registerHooks } from 'node:module';
registerHooks({
  load(url, context, next) {
    return url.endsWith('.css')
      ? { format: 'module', source: 'export {};', shortCircuit: true }
      : next(url, context);
  },
});
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';
const workspace = '11111111-1111-4111-8111-111111111111',
  url = `http://localhost/w/${workspace}/?view=chat`;
const session = {
  user: { id: 'owner', email: 'owner@example.test' },
  workspaces: [{ id: workspace, name: 'Store', role: 'Admin' }],
};
const response = (value, status = 200) => ({ ok: status < 400, status, json: async () => value });
const task = {
  id: 'reply',
  chat: 'channel',
  thread: null,
  source_message: 'input',
  sequence: 2,
  conversation_name: 'Operations',
  requester: 'owner',
  requester_name: 'Ananya',
  summary: 'Prepare the quote',
  state: 'input',
  latest_update: 'PRIVATE_SAVED_PROGRESS',
  created_ms: 1000,
  started_ms: 2000,
  finished_ms: null,
  can_cancel: false,
  cancellation: null,
  approvals: [],
  questions: [
    { id: 'q', title: 'Choose delivery', state: 'pending', respondent: 'owner', respondent_name: 'Ananya' },
  ],
  checked_ms: 3000,
  worker_released: true,
  source_scope: 'source-a',
};
async function setup() {
  const context = await import('../src/workspace-context.ts');
  await context.enterWorkspace();
  return import('../src/react/chat/task-progress.tsx');
}
test('progress is read only on explicit opening, shows saved waits, preserves failed checks and retries without new work', async () => {
  const calls = [];
  let fail = false;
  await reactFixture(
    async ({ render, button, click }) => {
      const { TaskProgressDialog } = await setup();
      await render(() => null);
      assert.equal(calls.length, 0);
      await render(TaskProgressDialog, { chat: 'channel', reply: 'reply', close: () => {} });
      assert.match(document.body.textContent, /PRIVATE_SAVED_PROGRESS/);
      assert.match(document.body.textContent, /Worker released/);
      assert.ok(button('Answer question'));
      assert.equal(button('Stop request'), undefined);
      fail = true;
      await click(button('Check now'));
      assert.match(document.body.textContent, /Showing the last successful check/);
      assert.match(document.body.textContent, /PRIVATE_SAVED_PROGRESS/);
      fail = false;
      await click(button('Check now'));
      assert.equal(document.querySelector('[role=alert]'), null);
      await render(() => null);
      assert.equal(document.querySelector('dialog'), null);
      assert.ok(calls.every((c) => c.method === 'GET'));
      assert.ok(calls.every((c) => c.path.endsWith('/chats/channel/messages/reply/progress')));
    },
    {
      url,
      fetch: async (path, options = {}) => {
        if (path.endsWith('/session')) return response(session);
        calls.push({ path, method: options.method });
        return fail ? response({ error: 'Network unavailable' }, 503) : response(task);
      },
    },
  );
});
test('denial clears saved details and retry, stale task changes and unmount reads stay fenced', async () => {
  let denied = false;
  const old = [];
  await reactFixture(
    async ({ render, button, click, update }) => {
      const { TaskProgressDialog } = await setup();
      const props = { chat: 'channel', reply: 'reply', close: () => {} };
      await render(TaskProgressDialog, props);
      assert.match(document.body.textContent, /PRIVATE_SAVED_PROGRESS/);
      denied = true;
      await click(button('Check now'));
      assert.doesNotMatch(document.body.textContent, /PRIVATE_SAVED_PROGRESS|Choose delivery/);
      assert.equal(button('Retry').disabled, false);
      denied = false;
      await click(button('Retry'));
      assert.match(document.body.textContent, /PRIVATE_SAVED_PROGRESS/);
      await render(TaskProgressDialog, { ...props, reply: 'old' });
      await render(TaskProgressDialog, { ...props, reply: 'new' });
      await update(() => old.forEach((fn) => fn(response({ ...task, latest_update: 'PRIVATE_OLD_TASK' }))));
      assert.match(document.body.textContent, /CURRENT_NEW_TASK/);
      assert.doesNotMatch(document.body.textContent, /PRIVATE_OLD_TASK/);
      await render(() => null);
      await update(() => old.forEach((fn) => fn(response(task))));
      assert.equal(document.body.textContent, '');
    },
    {
      url,
      fetch: async (path) => {
        if (path.endsWith('/session')) return response(session);
        if (path.includes('/old/')) return new Promise((resolve) => old.push(resolve));
        return denied
          ? response({ error: 'Task source unavailable' }, 403)
          : response({
              ...task,
              latest_update: path.includes('/new/') ? 'CURRENT_NEW_TASK' : task.latest_update,
            });
      },
    },
  );
});
test('exact task Stop guards double submission and retains an uncertain save without claiming cancellation', async () => {
  let finish;
  const writes = [];
  await reactFixture(
    async ({ render, button, update }) => {
      const { TaskProgressDialog } = await setup();
      await render(TaskProgressDialog, { chat: 'channel', reply: 'reply', close: () => {} });
      await update(() => {
        button('Stop request').click();
        button('Stop request').click();
      });
      assert.equal(writes.length, 1);
      assert.ok(writes[0].endsWith('/messages/reply/cancel'));
      await update(() => finish(response({ error: 'Stop acknowledgment unavailable' }, 503)));
      assert.match(document.body.textContent, /Stop acknowledgment unavailable/);
      assert.match(document.body.textContent, /PRIVATE_SAVED_PROGRESS/);
      assert.ok(button('Stop request'));
    },
    {
      url,
      fetch: async (path, options = {}) => {
        if (path.endsWith('/session')) return response(session);
        if (options.method === 'POST') {
          writes.push(path);
          return new Promise((resolve) => {
            finish = resolve;
          });
        }
        return response({ ...task, can_cancel: true });
      },
    },
  );
});
test('scoped live invalidation immediately clears an open progress view, fences late reads and cleans subscriptions', async () => {
  let stream,
    stopLive,
    revoked = false,
    hold = false;
  const late = [];
  let progressCalls = 0;
  const previous = globalThis.EventSource;
  globalThis.EventSource = class {
    constructor() {
      stream = this;
      this.listeners = new Map();
    }
    addEventListener(k, f) {
      this.listeners.set(k, f);
    }
    close() {}
  };
  try {
    await reactFixture(
      async ({ render, button, click, update }) => {
        try {
          const { TaskProgressDialog } = await setup();
          const { startLive } = await import('../src/react/chat/live.ts');
          await update(() => {
            stopLive = startLive();
          });
          await render(TaskProgressDialog, { chat: 'channel', reply: 'reply', close: () => {} });
          const emit = (event) =>
            stream.listeners.get('changes')({ data: JSON.stringify({ cursor: 42, events: [event] }) });
          const count = progressCalls;
          await update(() => emit({ kind: 'event_access', chat: 'channel', source: 'other-source' }));
          assert.equal(progressCalls, count);
          assert.match(document.body.textContent, /PRIVATE_SAVED_PROGRESS/);
          hold = true;
          await click(button('Check now'));
          revoked = true;
          await update(() => emit({ kind: 'event_access', chat: 'channel', source: 'source-a' }));
          assert.doesNotMatch(document.body.textContent, /PRIVATE_SAVED_PROGRESS|Choose delivery/);
          await update(() =>
            late.forEach((fn) => fn(response({ ...task, latest_update: 'PRIVATE_STALE_PROGRESS' }))),
          );
          assert.doesNotMatch(document.body.textContent, /PRIVATE_STALE_PROGRESS/);
          assert.ok(button('Retry'));
          await render(() => null);
          const after = progressCalls;
          await update(() => emit({ kind: 'reset' }));
          assert.equal(progressCalls, after);
        } finally {
          await update(() => stopLive?.());
        }
      },
      {
        url,
        fetch: async (path) => {
          if (path.endsWith('/session')) return response(session);
          if (path.endsWith('/message-capabilities')) return response({ live: true });
          if (path.endsWith('/progress')) {
            progressCalls++;
            if (revoked) return response({ error: 'Source denied' }, 403);
            if (hold) return new Promise((resolve) => late.push(resolve));
            return response(task);
          }
          return response({});
        },
      },
    );
  } finally {
    globalThis.EventSource = previous;
  }
});
