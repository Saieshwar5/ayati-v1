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
const gmail = {
  connection: 'gmail-connection',
  app: 'gmail',
  name: 'Gmail',
  account: 'accounts@example.test',
  state: 'ready',
  detail: null,
  read_title: 'Find emails',
  chat: 'channel',
  chat_name: 'Accounts',
  visibility: 'private',
  conversations: [{ id: 'channel', name: 'Accounts', visibility: 'private' }],
  authority: { connection_revision: 4, read_revision: 7, delegation_revision: null },
};
const calendar = {
  ...gmail,
  connection: 'calendar-connection',
  app: 'google_calendar',
  name: 'Google Calendar',
  account: null,
};
const newEmail = { id: 'gmail.new_email', title: 'New email', fields: ['from', 'subject', 'text'] };
const directory = (apps) => ({ choices: [], triggers: [], apps, execution_available: true });
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
  if (path.endsWith('/gmail-connection/events')) return response({ events: [newEmail] });
  if (path.endsWith('/calendar-connection/events'))
    return response({ events: [{ id: 'google_calendar.new_event', title: 'New calendar event', fields: ['title'] }] });
  if (path.endsWith('/runs')) return response({ runs: [] });
  return response({});
}

test('a trigger from an app reviews app, event, fields and audience before one exact activation', async () => {
  const writes = [];
  let finish;
  await reactFixture(
    async ({ render, button, click, change, update }) => {
      await setup();
      const { CreateAppEvent } = await import('../src/react/agents/automation/app-event-create.tsx');
      await render(CreateAppEvent, { agent: 'a', directory: directory([gmail]), close: () => {}, saved: async () => {} });
      assert.equal(field('Event').value, 'gmail.new_email');
      await change(field('Trigger name'), 'Invoices');
      await change(field('Task instructions'), 'Read the email and say whether it asks for payment.');
      await change(field('subject'), 'Invoice');
      await click(field('Activity, and post the result to'));
      await click(field('Remember earlier runs'));
      await click(button('Review event trigger'));
      assert.equal(writes.length, 0);
      for (const text of [
        'Activity, and a summary in Accounts',
        'Remember earlier runs',
        'Gmail',
        'accounts@example.test',
        'New email',
        'from, subject, text',
        'Accounts',
        'owner@example.test',
        'subject = Invoice',
        '“Find emails”',
      ])
        assert.ok(document.body.textContent.includes(text), text);
      await update(() => {
        button('Activate event trigger').click();
        button('Activate event trigger').click();
      });
      assert.equal(writes.length, 1);
      assert.equal(writes[0].connection, 'gmail-connection');
      assert.equal(writes[0].event, 'gmail.new_email');
      assert.equal(writes[0].chat, 'channel');
      assert.equal(writes[0].memory, 'remember');
      assert.deepEqual(writes[0].filter, { subject: 'Invoice' });
      assert.equal(writes[0].connection_revision, 4);
      assert.equal(writes[0].read_revision, 7);
      assert.equal(writes[0].source, undefined);
      await update(() => finish(response({ id: writes[0].id })));
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

test('an app trigger keeps results in Activity and starts each run fresh unless asked', async () => {
  const writes = [];
  await reactFixture(
    async ({ render, button, click, change, update }) => {
      await setup();
      const { CreateAppEvent } = await import('../src/react/agents/automation/app-event-create.tsx');
      await render(CreateAppEvent, { agent: 'a', directory: directory([gmail]), close: () => {}, saved: async () => {} });
      assert.equal(field('Activity only').checked, true);
      assert.equal(field('Fresh each run').checked, true);
      assert.equal(field('Conversation'), undefined, 'no conversation is asked for by default');
      await change(field('Trigger name'), 'Invoices');
      await change(field('Task instructions'), 'Say whether it asks for payment.');
      await click(button('Review event trigger'));
      for (const text of ['Activity only', 'Fresh each run', 'never reads anyone’s conversations'])
        assert.ok(document.body.textContent.includes(text), text);
      await update(() => button('Activate event trigger').click());
      assert.equal(writes.length, 1);
      assert.equal('chat' in writes[0], false);
      assert.equal('memory' in writes[0], false);
    },
    {
      url,
      fetch: async (path, init = {}) => {
        if (init.method === 'POST') {
          writes.push(JSON.parse(init.body));
          return new Promise(() => {});
        }
        return base(path);
      },
    },
  );
});

test('apps that are not ready explain why and never load events or write', async () => {
  const reads = [];
  let writes = 0;
  const notSetUp = {
    ...gmail,
    state: 'unavailable',
    detail: 'App events aren’t set up on this server yet. A Cuckoo administrator needs to set them up.',
  };
  const reconnect = {
    ...calendar,
    state: 'reconnect',
    detail: 'This account needs reconnecting before its events can start work. An admin can reconnect it on Apps.',
  };
  await reactFixture(
    async ({ render, button, change }) => {
      await setup();
      const { CreateAppEvent } = await import('../src/react/agents/automation/app-event-create.tsx');
      await render(CreateAppEvent, {
        agent: 'a',
        directory: directory([notSetUp, reconnect]),
        close: () => {},
        saved: async () => {},
      });
      assert.match(document.body.textContent, /aren’t set up on this server/);
      assert.equal(button('Review event trigger').disabled, true);
      await change(field('App'), 'calendar-connection');
      assert.match(document.body.textContent, /needs reconnecting/);
      assert.equal(button('Review event trigger').disabled, true);
      assert.equal(field('Event'), undefined);
      assert.equal(reads.filter((p) => p.includes('/apps/')).length, 0);
      assert.equal(writes, 0);
      // The trigger list says the same without opening anything.
      const { EventTriggers } = await import('../src/react/agents/automation/events.tsx');
      await render(EventTriggers, { agent: 'a' });
      assert.match(document.body.textContent, /Gmail: App events aren’t set up/);
      assert.match(document.body.textContent, /Google Calendar: This account needs reconnecting/);
    },
    {
      url,
      fetch: async (path, init = {}) => {
        if (init.method === 'POST') writes++;
        reads.push(path);
        if (path.endsWith('/event-triggers')) return response(directory([notSetUp, reconnect]));
        return base(path);
      },
    },
  );
});

test('a late event list for the previously chosen app never replaces the current one', async () => {
  let finishGmail;
  await reactFixture(
    async ({ render, change, update }) => {
      await setup();
      const { CreateAppEvent } = await import('../src/react/agents/automation/app-event-create.tsx');
      await render(CreateAppEvent, {
        agent: 'a',
        directory: directory([gmail, calendar]),
        close: () => {},
        saved: async () => {},
      });
      await change(field('App'), 'calendar-connection');
      assert.equal(field('Event').value, 'google_calendar.new_event');
      await update(() => finishGmail(response({ events: [newEmail] })));
      assert.equal(field('Event').value, 'google_calendar.new_event');
      assert.ok(!document.body.textContent.includes('New email'));
    },
    {
      url,
      fetch: async (path) =>
        path.endsWith('/gmail-connection/events')
          ? new Promise((resolve) => {
              finishGmail = resolve;
            })
          : base(path),
    },
  );
});

test('an unacknowledged app activation keeps its exact request across reopening', async () => {
  const writes = [];
  await reactFixture(
    async ({ render, button, click, change }) => {
      await setup();
      const { CreateAppEvent } = await import('../src/react/agents/automation/app-event-create.tsx');
      const props = { agent: 'a', directory: directory([gmail]), close: () => {}, saved: async () => {} };
      await render(CreateAppEvent, props);
      await change(field('Trigger name'), 'Invoices');
      await change(field('Task instructions'), 'Check the email.');
      await click(button('Review event trigger'));
      await click(button('Activate event trigger'));
      assert.match(document.body.textContent, /Unacknowledged save/);
      await render(() => createElement('p', null, 'Closed'));
      await render(CreateAppEvent, props);
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

test('removing a trigger asks first and sends one exact removal', async () => {
  const writes = [];
  let finish,
    closed = 0;
  const trigger = {
    id: 'rule',
    source: 'subscription',
    chat: 'channel',
    name: 'Invoices',
    instructions: 'Check the email.',
    source_definition: { id: 'subscription', name: 'New email · Gmail', event_type: 'gmail.new_email', fields: ['from'], binding: 'b' },
    filter: {},
    state: 'enabled',
    revision: 3,
    can_resume: true,
    principal_name: 'Ananya',
    chat_name: 'Accounts',
    detail: '',
  };
  await reactFixture(
    async ({ render, button, click, update }) => {
      await setup();
      const { EventReview } = await import('../src/react/agents/automation/event-review.tsx');
      await render(EventReview, { agent: 'a', trigger, close: () => closed++, saved: async () => {} });
      await click(button('Remove event trigger'));
      assert.equal(writes.length, 0);
      assert.match(document.body.textContent, /stops sending it/);
      await update(() => {
        button('Remove for good').click();
        button('Remove for good').click();
      });
      assert.equal(writes.length, 1);
      assert.deepEqual(
        { remove: writes[0].remove, enabled: writes[0].enabled, revision: writes[0].revision },
        { remove: true, enabled: false, revision: 3 },
      );
      await update(() => finish(response({ state: 'removed' })));
      assert.equal(closed, 1);
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
        return path.endsWith('/rule') ? response(trigger) : base(path);
      },
    },
  );
});
