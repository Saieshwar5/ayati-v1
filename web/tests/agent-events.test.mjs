import { registerHooks } from 'node:module';
registerHooks({
  load(url, c, next) {
    return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, c);
  },
});
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { reactFixture } from './react-fixture.mjs';
const w = '11111111-1111-4111-8111-111111111111',
  url = `http://localhost/w/${w}/?view=agents`;
const response = (value, status = 200) => ({ ok: status < 400, status, json: async () => value });
const choice = {
  source: {
    id: 'source',
    name: 'Order changed',
    event_type: 'ORDER_CHANGED',
    fields: ['order_id', 'status'],
    binding: 'reviewed-binding',
  },
  chat: 'channel',
  chat_name: 'Delivery operations',
  visibility: 'private',
  account: 'orders@example.test',
  authority: { connection_revision: 2, read_revision: 3, delegation_revision: null },
};
const directory = { choices: [choice], triggers: [], execution_available: true };
const trigger = {
  id: 'rule',
  source: 'source',
  chat: 'channel',
  name: 'Review delays',
  instructions: 'PRIVATE_EVENT_TASK',
  source_definition: choice.source,
  filter: { status: 'delayed' },
  state: 'enabled',
  revision: 5,
  can_resume: true,
  choice,
  principal_name: 'Ananya',
  chat_name: choice.chat_name,
  detail: '',
};
const field = (label) =>
  [...document.querySelectorAll('label')]
    .find((n) => n.textContent.startsWith(label))
    ?.querySelector('input,textarea,select');
async function setup() {
  const context = await import('../src/workspace-context.ts');
  await context.enterWorkspace();
}
function base(path) {
  if (path.endsWith('/session'))
    return response({
      user: { id: 'owner', email: 'owner@example.test' },
      workspaces: [{ id: w, name: 'Store', role: 'Admin' }],
    });
  if (path.endsWith('/runs')) return response({ runs: [] });
  return response(trigger);
}
test('event review shows account projection audience and principal before an exact single explicit activation', async () => {
  const writes = [];
  let finish;
  await reactFixture(
    async ({ render, button, click, change, update }) => {
      await setup();
      const { CreateEvent } = await import('../src/react/agents/automation/event-create.tsx');
      await render(CreateEvent, { agent: 'a', directory, close: () => {}, saved: async () => {} });
      await change(field('Trigger name'), 'Review delays');
      await change(field('Task instructions'), 'Review this event and ask before changing orders.');
      await change(field('status'), 'delayed');
      await click(button('Review event trigger'));
      assert.equal(writes.length, 0);
      for (const text of [
        'orders@example.test',
        'order_id, status',
        'Delivery operations',
        'owner@example.test',
        'status = delayed',
        'current members',
      ])
        assert.ok(document.body.textContent.includes(text));
      await update(() => {
        button('Activate event trigger').click();
        button('Activate event trigger').click();
      });
      assert.equal(writes.length, 1);
      assert.deepEqual(writes[0].filter, { status: 'delayed' });
      assert.equal(writes[0].chat, 'channel');
      assert.equal(writes[0].connection_revision, 2);
      assert.equal(writes[0].read_revision, 3);
      await update(() => finish(response(trigger)));
      assert.match(document.body.textContent, /Event trigger saved/);
    },
    {
      url,
      fetch: async (path, init = {}) => {
        if (init.method === 'POST') {
          writes.push(JSON.parse(init.body));
          return new Promise((resolve) => {
            finish = resolve;
          });
        }
        return base(path);
      },
    },
  );
});
test('uncertain event activation preserves its exact ID and payload across reopening', async () => {
  const writes = [];
  await reactFixture(
    async ({ render, button, click, change }) => {
      await setup();
      const { CreateEvent } = await import('../src/react/agents/automation/event-create.tsx');
      const props = { agent: 'a', directory, close: () => {}, saved: async () => {} };
      await render(CreateEvent, props);
      await change(field('Trigger name'), 'Delay review');
      await change(field('Task instructions'), 'Check the supplied fields.');
      await click(button('Review event trigger'));
      await click(button('Activate event trigger'));
      assert.match(document.body.textContent, /Unacknowledged save/);
      await render(() => createElement('p', null, 'Closed'));
      await render(CreateEvent, props);
      await click(button('Retry same save'));
      assert.deepEqual(writes[0], writes[1]);
    },
    {
      url,
      fetch: async (path, init = {}) => {
        if (init.method === 'POST') {
          writes.push(JSON.parse(init.body));
          return response({ error: 'Unacknowledged save' }, 503);
        }
        return base(path);
      },
    },
  );
});
test('trigger review fetches instructions without mutation and pauses with one exact revisioned request', async () => {
  const writes = [];
  let finish;
  await reactFixture(
    async ({ render, button, update }) => {
      await setup();
      const { EventReview } = await import('../src/react/agents/automation/event-review.tsx');
      await render(EventReview, { agent: 'a', trigger, close: () => {}, saved: async () => {} });
      assert.match(document.body.textContent, /PRIVATE_EVENT_TASK/);
      assert.equal(writes.length, 0);
      await update(() => {
        button('Pause event trigger').click();
        button('Pause event trigger').click();
      });
      assert.equal(writes.length, 1);
      assert.equal(writes[0].enabled, false);
      assert.equal(writes[0].revision, 5);
      await update(() => finish(response({ state: 'paused' })));
    },
    {
      url,
      fetch: async (path, init = {}) => {
        if (init.method === 'POST') {
          writes.push(JSON.parse(init.body));
          return new Promise((resolve) => {
            finish = resolve;
          });
        }
        return base(path);
      },
    },
  );
});
test('unavailable provider setup explains the missing source and starts no write', async () => {
  let writes = 0;
  await reactFixture(
    async ({ render, button }) => {
      await setup();
      const { EventTriggers } = await import('../src/react/agents/automation/events.tsx');
      await render(EventTriggers, { agent: 'a' });
      assert.equal(button('Create event trigger').disabled, true);
      assert.match(document.body.textContent, /operator must configure/);
      assert.match(document.body.textContent, /no model calls/);
      assert.equal(writes, 0);
    },
    {
      url,
      fetch: async (path, init = {}) => {
        if (init.method === 'POST') writes++;
        return path.endsWith('/event-triggers') ? response({ ...directory, choices: [] }) : base(path);
      },
    },
  );
});

test('editing an uncertain activation explicitly creates a new request and retains exact text filters', async () => {
  const writes = [];
  await reactFixture(
    async ({ render, button, click, change }) => {
      await setup();
      const { CreateEvent } = await import('../src/react/agents/automation/event-create.tsx');
      const props = { agent: 'a', directory, close: () => {}, saved: async () => {} };
      await render(CreateEvent, props);
      await change(field('Trigger name'), 'First delay trigger');
      await change(field('Task instructions'), 'Review the event.');
      await change(field('status'), 'delayed');
      await click(button('Review event trigger'));
      await click(button('Activate event trigger'));
      await render(() => createElement('p', null, 'Closed'));
      await render(CreateEvent, props);
      await click(button('Retry same save'));
      assert.deepEqual(writes[0], writes[1]);
      await click(button('Edit as a new trigger'));
      assert.equal(field('status').value, 'delayed');
      await change(field('Trigger name'), 'Different explicit trigger');
      await click(button('Review event trigger'));
      await click(button('Activate event trigger'));
      assert.notEqual(writes[0].id, writes[2].id);
      assert.deepEqual(writes[2].filter, { status: 'delayed' });
    },
    {
      url,
      fetch: async (path, init = {}) => {
        if (init.method === 'POST') {
          writes.push(JSON.parse(init.body));
          return response({ error: 'Unacknowledged save' }, 503);
        }
        return base(path);
      },
    },
  );
});

test('late event-state acknowledgments after unmount cannot refresh a previous agent', async () => {
  let finish,
    refreshes = 0;
  await reactFixture(
    async ({ render, button, click, update }) => {
      await setup();
      const { EventReview } = await import('../src/react/agents/automation/event-review.tsx');
      await render(EventReview, {
        agent: 'a',
        trigger,
        close: () => {},
        saved: async () => {
          refreshes++;
        },
      });
      await click(button('Pause event trigger'));
      await render(() => createElement('p', null, 'Another workspace'));
      await update(() => finish(response({ state: 'paused' })));
      assert.equal(refreshes, 0);
      assert.equal(document.body.textContent, 'Another workspace');
    },
    {
      url,
      fetch: async (path, init = {}) =>
        init.method === 'POST'
          ? new Promise((resolve) => {
              finish = resolve;
            })
          : base(path),
    },
  );
});
