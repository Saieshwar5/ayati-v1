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
  url = `http://localhost/w/${workspace}/?view=home`;
const session = {
  user: { id: 'owner', email: 'owner@example.test' },
  workspaces: [{ id: workspace, name: 'Store', role: 'Admin' }],
};
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const definition = {
  title: 'PRIVATE_PANEL_TITLE',
  purpose: 'Follow-up',
  template: 'brief',
  sections: [{ kind: 'text', heading: 'Summary', body: 'APPROVED_BODY' }],
};
const snapshot = {
  revision: 1,
  available: true,
  definition,
  saved_ms: 1000,
  source_operations: 0,
  chat: 'channel',
  thread: null,
  task: 'reply',
};
const panel = {
  id: '00000000-0000-4000-8000-000000000001',
  revision: 2,
  active_revision: 1,
  agent_name: 'Nova',
  active: snapshot,
  draft: {
    ...snapshot,
    revision: 2,
    definition: { ...definition, sections: [{ kind: 'text', heading: 'Draft', body: 'NEW_DRAFT_BODY' }] },
  },
};
const page = { available: true, panels: [panel], layout: { revision: 0, order: [], wide: [], hidden: [] } };
test('routine saved-data refresh preserves an open update-agreement draft', async () => {
  let current = structuredClone(page);
  current.panels[0].draft = null;
  current.panels[0].maintenance = {
    revision: 1, panel_revision: 1, enabled: true, eligible: true, execution_available: true,
    instructions: 'Saved instructions', interval_seconds: 900, stale_seconds: 1800,
    scopes: [], status: 'current', checked_ms: 1000, saved_ms: 1000, next_ms: 100000,
  };
  await reactFixture(async ({render, button, click, change, update}) => {
    const {WorkspacePanels} = await setup();
    await render(WorkspacePanels);
    await click(button('Panel updates'));
    await change(document.querySelector('dialog textarea'), 'Keep my unsaved update instructions');
    current.panels[0].active.generation = 2;
    current.panels[0].active.definition.sections[0].body = 'A_NEW_ROUTINE_SNAPSHOT';
    await update(() => window.dispatchEvent(new window.Event('focus')));
    assert.match(document.body.textContent, /A_NEW_ROUTINE_SNAPSHOT/);
    assert.equal(document.querySelector('dialog textarea').value, 'Keep my unsaved update instructions');
  }, {url, fetch: async path => path.endsWith('/session') ? response(session) : response(current)});
});
async function setup() {
  const c = await import('../src/workspace-context.ts');
  await c.enterWorkspace();
  return import('../src/react/home-panels/panels.tsx');
}
test('Home reads saved snapshots without writes, keeps the approved view during draft review and reuses an exact failed review', async () => {
  const writes = [];
  let finish;
  await reactFixture(
    async ({ render, button, click, update }) => {
      const { WorkspacePanels } = await setup();
      await render(WorkspacePanels);
      assert.match(document.body.textContent, /APPROVED_BODY/);
      assert.doesNotMatch(document.body.textContent, /NEW_DRAFT_BODY/);
      assert.equal(writes.length, 0);
      await click(button('Review draft 2'));
      assert.match(document.querySelector('dialog').textContent, /NEW_DRAFT_BODY/);
      assert.match(document.body.textContent, /APPROVED_BODY/);
      await update(() => {
        button('Keep on Home').click();
        button('Keep on Home').click();
      });
      assert.equal(writes.length, 1);
      await update(() => finish(response({ error: 'Review acknowledgment lost' }, 503)));
      assert.match(document.querySelector('dialog').textContent, /Review acknowledgment lost/);
      assert.match(document.querySelector('dialog').textContent, /NEW_DRAFT_BODY/);
      await click(button('Keep on Home'));
      assert.equal(writes.length, 2);
      assert.deepEqual(writes[0].body, writes[1].body);
      assert.equal(writes[0].body.revision, 2);
      await update(() => finish(response({ id: panel.id, active_revision: 2 })));
      assert.equal(document.querySelector('dialog'), null);
    },
    {
      url,
      fetch: async (path, options = {}) => {
        if (path.endsWith('/session')) return response(session);
        if (options.method === 'POST') {
          writes.push({ path, body: JSON.parse(options.body) });
          return new Promise((resolve) => {
            finish = resolve;
          });
        }
        return response(page);
      },
    },
  );
});
test('personal arrangement sends exact revisions and hiding never issues a task or publication mutation', async () => {
  const writes = [];
  let current = structuredClone(page);
  await reactFixture(
    async ({ render, button, click }) => {
      const { WorkspacePanels } = await setup();
      await render(WorkspacePanels);
      await click(button('Arrange'));
      await click(button('Hide from my Home'));
      assert.equal(writes.length, 1);
      assert.ok(writes[0].path.endsWith('/home-panels/layout'));
      assert.equal(writes[0].body.revision, 0);
      assert.deepEqual(writes[0].body.hidden, [panel.id]);
      assert.doesNotMatch(document.body.textContent, /APPROVED_BODY/);
      await click(button('Hidden panels'));
      assert.match(document.body.textContent, /APPROVED_BODY/);
      await click(button('Show on my Home'));
      assert.equal(writes[1].body.revision, 1);
      assert.deepEqual(writes[1].body.hidden, []);
    },
    {
      url,
      fetch: async (path, options = {}) => {
        if (path.endsWith('/session')) return response(session);
        if (options.method === 'POST') {
          const body = JSON.parse(options.body);
          writes.push({ path, body });
          current.layout = { ...body, revision: body.revision + 1 };
          return response(current.layout);
        }
        return response(current);
      },
    },
  );
});
test('source invalidation clears saved Home content immediately, fences late reads and cleans personal subscriptions', async () => {
  let stream,
    stopLive,
    revoked = false,
    hold = false;
  const late = [];
  let reads = 0;
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
          const { WorkspacePanels } = await setup();
          const { startLive } = await import('../src/react/chat/live.ts');
          await update(() => {
            stopLive = startLive();
          });
          await render(WorkspacePanels);
          assert.match(document.body.textContent, /PRIVATE_PANEL_TITLE/);
          hold = true;
          await update(() => window.dispatchEvent(new window.Event('focus')));
          revoked = true;
          await update(() =>
            stream.listeners.get('changes')({
              data: JSON.stringify({ cursor: 42, events: [{ kind: 'panel_access' }] }),
            }),
          );
          assert.doesNotMatch(document.body.textContent, /PRIVATE_PANEL_TITLE|APPROVED_BODY/);
          await update(() => late.forEach((fn) => fn(response(page))));
          assert.doesNotMatch(document.body.textContent, /PRIVATE_PANEL_TITLE|APPROVED_BODY/);
          assert.ok(button('Retry panels'));
          await render(() => null);
          const count = reads;
          await update(() =>
            stream.listeners.get('changes')({
              data: JSON.stringify({ cursor: 43, events: [{ kind: 'panel_access' }] }),
            }),
          );
          assert.equal(reads, count);
        } finally {
          await update(() => stopLive?.());
        }
      },
      {
        url,
        fetch: async (path) => {
          if (path.endsWith('/session')) return response(session);
          if (path.endsWith('/message-capabilities')) return response({ live: true });
          if (path.endsWith('/home-panels')) {
            reads++;
            if (revoked) return response({ error: 'Source unavailable' }, 403);
            if (hold) return new Promise((resolve) => late.push(resolve));
            return response(page);
          }
          return response({});
        },
      },
    );
  } finally {
    globalThis.EventSource = previous;
  }
});
test('asking for a panel is explicit, keeps the draft on a failed send and reuses the exact message identity', async () => {
  const writes = [];
  let finish;
  await reactFixture(
    async ({ render, button, click, change, submit, update, navigation }) => {
      await setup();
      const { AskPanel } = await import('../src/react/home-panels/create.tsx');
      await render(AskPanel, { close: () => {} });
      assert.equal(writes.length, 0);
      await change(document.querySelector('select'), 'agent');
      await change(document.querySelectorAll('select')[1], 'channel');
      await change(document.querySelector('textarea'), 'Prepare my follow-up view.');
      const form = document.querySelector('form');
      await submit(form);
      await submit(form);
      assert.equal(writes.length, 1);
      await update(() => finish(response({ error: 'Send acknowledgment unavailable' }, 503)));
      assert.equal(document.querySelector('textarea').value, 'Prepare my follow-up view.');
      await submit(form);
      assert.equal(writes[0].body.id, writes[1].body.id);
      assert.equal(writes[1].body.agent, 'agent');
      await update(() => finish(response({ ok: true })));
      assert.ok(navigation[0].includes('conversation=channel'));
      assert.ok(writes.every((w) => w.path.endsWith('/chats/channel/messages')));
    },
    {
      url,
      fetch: async (path, options = {}) => {
        if (path.endsWith('/session')) return response(session);
        if (options.method === 'POST') {
          writes.push({ path, body: JSON.parse(options.body) });
          return new Promise((resolve) => {
            finish = resolve;
          });
        }
        return path.endsWith('/chat-directory')
          ? response({
              users: [],
              agents: [
                { id: 'agent', name: 'Nova', audience: 'selected' },
                { id: 'private', name: 'Private', audience: 'only_me' },
              ],
              role: 'Admin',
            })
          : response({
              chats: [{ id: 'channel', name: 'Operations', kind: 'channel', agents: ['agent'], users: [] }],
            });
      },
    },
  );
});
test('agent content stays inert and finite extreme chart values produce bounded SVG coordinates', async () => {
  await reactFixture(async ({ render }) => {
    const { PanelContent } = await import('../src/react/home-panels/snapshot.tsx');
    await render(PanelContent, {
      definition: {
        ...definition,
        title: '<img src=x onerror=alert(1)>',
        sections: [
          {
            kind: 'chart',
            title: 'Range',
            points: [
              { label: 'Low', value: -Number.MAX_VALUE },
              { label: 'High', value: Number.MAX_VALUE },
            ],
          },
        ],
      },
    });
    assert.equal(document.querySelector('img'), null);
    assert.doesNotMatch(document.querySelector('polyline').getAttribute('points'), /NaN|Infinity/);
    assert.match(document.body.textContent, /<img src=x onerror=alert\(1\)>/);
  });
});

test('an explicit panel change stays in its original agent/conversation, preserves a failed draft and blocks a stale source before sending work', async () => {
  const writes = [];
  let stale = true;
  await reactFixture(
    async ({ render, button, change, submit }) => {
      await setup();
      const { AskPanel } = await import('../src/react/home-panels/create.tsx');
      await render(AskPanel, {
        close: () => {},
        target: { id: panel.id, revision: 2, agent: 'agent', chat: 'channel' },
      });
      assert.ok(document.querySelector('select').disabled);
      assert.equal(document.querySelectorAll('select')[1].value, 'channel');
      await change(document.querySelector('textarea'), 'Change the follow-up presentation.');
      await submit(document.querySelector('form'));
      assert.equal(writes.length, 0);
      assert.match(document.body.textContent, /selected panel or source changed/);
      assert.equal(document.querySelector('textarea').value, 'Change the follow-up presentation.');
      stale = false;
      await submit(document.querySelector('form'));
      assert.equal(writes.length, 1);
      assert.equal(writes[0].agent, 'agent');
      assert.ok(writes[0].content.includes(`Revise personal panel ${panel.id} using expected_revision=2`));
    },
    {
      url,
      fetch: async (path, options = {}) => {
        if (path.endsWith('/session')) return response(session);
        if (options.method === 'POST') {
          writes.push(JSON.parse(options.body));
          return response({ error: 'Retain the draft for retry' }, 503);
        }
        if (path.endsWith('/home-panels'))
          return response({ ...page, panels: [{ ...panel, agent: 'agent', revision: stale ? 3 : 2 }] });
        return path.endsWith('/chat-directory')
          ? response({
              agents: [{ id: 'agent', name: 'Nova', audience: 'selected' }],
              users: [],
              role: 'Admin',
            })
          : response({
              chats: [{ id: 'channel', name: 'Operations', kind: 'channel', agents: ['agent'], users: [] }],
            });
      },
    },
  );
});
