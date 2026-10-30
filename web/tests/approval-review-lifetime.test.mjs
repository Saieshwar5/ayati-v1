import { registerHooks } from 'node:module';
registerHooks({
  load(url, context, next) {
    return url.endsWith('.css')
      ? { format: 'module', source: 'export {};', shortCircuit: true }
      : next(url, context);
  },
});
import { createElement } from 'react';
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
const approval = {
  id: 'action',
  title: 'Update order',
  agent_name: 'Nova',
  account_name: 'Orders',
  requester_name: 'Ananya',
  conversation_name: 'Work',
  approver_name: 'Ananya',
  chat: 'source',
  source_scope: null,
  state: 'pending',
  can_decide: true,
  arguments: { address: 'PRIVATE_EXACT_FIELDS' },
};
async function setup() {
  const c = await import('../src/workspace-context.ts');
  await c.enterWorkspace();
  const { ApprovalViewer } = await import('../src/react/approvals/viewer.tsx');
  const { ApprovalCard } = await import('../src/react/approvals/approval-card.tsx');
  return function Harness({ visible = true }) {
    return createElement(
      ApprovalViewer,
      null,
      visible
        ? createElement(ApprovalCard, { reference: { id: 'action', state: 'pending' } })
        : createElement('p', null, 'Checking conversation access…'),
    );
  };
}
test('an open exact approval remains owned by the conversation while its message reference refreshes', async () => {
  let writes = 0;
  await reactFixture(
    async ({ render, button, click }) => {
      const Harness = await setup();
      await render(Harness);
      await click(button('Review saved request'));
      assert.match(document.querySelector('dialog').textContent, /PRIVATE_EXACT_FIELDS/);
      await render(Harness, { visible: false });
      assert.ok(document.querySelector('dialog').open);
      assert.ok(button('Approve this action'));
      await render(Harness);
      assert.ok(document.querySelector('dialog').open);
      assert.equal(writes, 0);
      await click(button('Close'));
      assert.equal(document.querySelector('dialog'), null);
    },
    {
      url,
      fetch: async (path, options = {}) => {
        if (options.method === 'POST') writes++;
        return response(path.endsWith('/session') ? session : approval);
      },
    },
  );
});
test('access rechecks clear exact fields, deny actions and fence late reads even when the message row disappears', async () => {
  let stream,
    stopLive,
    hold = false,
    revoked = false;
  const old = [];
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
          const Harness = await setup();
          const { startLive } = await import('../src/react/chat/live.ts');
          await update(() => {
            stopLive = startLive();
          });
          await render(Harness);
          await click(button('Review saved request'));
          assert.match(document.body.textContent, /PRIVATE_EXACT_FIELDS/);
          hold = true;
          await click(button('Refresh request'));
          revoked = true;
          await render(Harness, { visible: false });
          await update(() =>
            stream.listeners.get('changes')({
              data: JSON.stringify({ cursor: 7, events: [{ kind: 'access' }] }),
            }),
          );
          assert.ok(document.querySelector('dialog').open);
          assert.doesNotMatch(document.body.textContent, /PRIVATE_EXACT_FIELDS/);
          assert.equal(button('Approve this action'), undefined);
          assert.match(document.body.textContent, /Source access denied/);
          await update(() =>
            old.forEach((fn) =>
              fn(response({ ...approval, arguments: { address: 'PRIVATE_STALE_FIELDS' } })),
            ),
          );
          assert.doesNotMatch(document.body.textContent, /PRIVATE_EXACT_FIELDS|PRIVATE_STALE_FIELDS/);
          await render(() => null);
          assert.equal(document.querySelector('dialog'), null);
        } finally {
          await update(() => stopLive?.());
        }
      },
      {
        url,
        fetch: async (path) => {
          if (path.endsWith('/session')) return response(session);
          if (path.endsWith('/message-capabilities')) return response({ live: true });
          if (path.includes('/control/actions/')) {
            if (revoked) return response({ error: 'Source access denied' }, 403);
            if (hold) return new Promise((resolve) => old.push(resolve));
            return response(approval);
          }
          return response({});
        },
      },
    );
  } finally {
    globalThis.EventSource = previous;
  }
});
