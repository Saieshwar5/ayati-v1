import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID, createHmac } from 'node:crypto';
import { createRequire } from 'node:module';
import { mkdtemp, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { remoteControlMcp } from './fixtures/remote_control_mcp.mjs';
import { until, root } from './fixtures/goose_app.mjs';
const { chromium } = createRequire(join(root, 'web/package.json'))('@playwright/test');
const secret = 'PRIVATE_LOCAL_EVENT_SIGNING_SECRET';
const payload = (id, status = 'delayed') => ({
  id,
  type: 'composio.trigger.message',
  metadata: {
    user_id: 'event-user',
    connected_account_id: 'event-account',
    auth_config_id: 'event-auth',
    trigger_id: 'event-trigger',
    trigger_slug: 'ORDER_CHANGED',
  },
  data: { order_id: 'PRIVATE_EVENT_ORDER', status, excluded: 'NEVER_PROJECT_THIS_FIELD' },
  timestamp: '2026-10-26T00:00:00Z',
});

test(
  'authenticated connected events create one durable task, release a real Goose question and retain independent exact approvals across restart',
  {
    skip: !process.env.RELAY_TEST_DATABASE_URL,
    timeout: 240000,
  },
  async (t) => {
    let app, browser;
    const pages = [],
      errors = [];
    t.after(async () => {
      try {
        if (app && !t.passed)
          for (const [i, p] of pages.entries()) {
            await p.screenshot({ path: join(app.data, `events-failure-${i}.png`), timeout: 5000 }).catch(() => {});
            await writeFile(join(app.data, `events-failure-${i}.html`), await p.content()).catch(() => {});
          }
      } finally {
        await browser?.close();
      }
    });
    const remote = await remoteControlMcp();
    t.after(() => remote.close());
    const dir = await mkdtemp(join(root, '.tmp/event-profile-')),
      profiles = join(dir, 'servers.json'),
      sources = join(dir, 'sources.json'),
      key = join(dir, 'signing.key'),
      bearer = join(dir, 'bearer');
    for (const [file, text] of [
      [profiles, '[]'],
      [sources, '[]'],
      [key, secret],
      [bearer, remote.token],
    ])
      await writeFile(file, text, { mode: 0o600 });
    app = await workerFixture(t, {
      mode: 'pooled',
      control: true,
      capacity: 1,
      workspaceLimit: 1,
      modelLimit: 1,
      leaseSeconds: 15,
      apiEnv: { AYATI_MCP_SERVERS_FILE: profiles, AYATI_EVENT_SOURCES_FILE: sources },
    });
    const w = app.path.split('/').at(-1),
      owner = (await app.request('/api/identity/session', app.cookie)).value.user;
    const member = await app.login(1),
      person = (await app.request('/api/identity/session', member)).value.user;
    await app.request(`${app.path}/chat-grants`, app.cookie, { user: person.id, agent: app.profile.id, allowed: true });
    const chat = (
      await app.request(`${app.path}/chats`, app.cookie, {
        id: randomUUID(),
        kind: 'channel',
        name: 'Connected delivery operations',
        users: [person.id],
        agents: [app.profile.id],
      })
    ).value.id;
    const connection = randomUUID();
    await writeFile(
      profiles,
      JSON.stringify([
        {
          id: 'orders',
          workspace: w,
          name: 'Orders workspace',
          account: 'orders@example.test',
          url: remote.url,
          bearer_file: bearer,
          operations: remote.operations,
        },
      ]),
    );
    await app.request(`${app.path}/control/systems`, app.cookie, { server: 'orders', approved: true, revision: 0 });
    await app.request(`${app.path}/control/connections`, app.cookie, {
      id: connection,
      server: 'orders',
      enabled: true,
      revision: 0,
    });
    const control = `${app.path}/agent-profiles/${app.profile.id}/control`;
    for (const [operation, mode] of [
      ['order.read', 'allow'],
      ['order.update', 'ask'],
    ])
      await app.request(`${control}/rules`, app.cookie, { connection, operation, mode, revision: 0, chat });
    const source = {
      id: randomUUID(),
      workspace: w,
      connection,
      connection_fingerprint: app.sql(`SELECT fingerprint FROM relay_control_connections WHERE id='${connection}'`),
      name: 'Reviewed order change',
      provider: 'composio_v3',
      user_id: 'event-user',
      connected_account_id: 'event-account',
      auth_config_id: 'event-auth',
      trigger_id: 'event-trigger',
      trigger_slug: 'ORDER_CHANGED',
      read_operation: 'order.read',
      fields: ['order_id', 'status'],
      secret_file: key,
    };
    await writeFile(sources, JSON.stringify([source]));
    const path = `${app.path}/agent-profiles/${app.profile.id}/event-triggers`,
      hook = `/hooks/composio/workspaces/${w}/${source.id}`;
    async function deliver(id, status = 'delayed', changes = {}) {
      const data = changes.data ?? payload(id, status),
        raw = changes.raw ?? JSON.stringify(data),
        time = String(changes.time ?? Math.floor(Date.now() / 1000)),
        headerId = changes.id ?? id;
      const signature = createHmac('sha256', secret).update(`${headerId}.${time}.${raw}`).digest('base64');
      return fetch(app.base + hook, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'webhook-id': headerId,
          'webhook-timestamp': time,
          'webhook-signature': changes.signature ?? `v1,${signature}`,
        },
        body: raw,
      });
    }
    assert.equal((await deliver('bad-signature', 'delayed', { signature: 'v1,invalid' })).status, 401);
    assert.equal((await deliver('stale', 'delayed', { time: Math.floor(Date.now() / 1000) - 301 })).status, 401);
    const forged = payload('forged');
    forged.metadata.connected_account_id = 'other-account';
    assert.equal((await deliver('forged', 'delayed', { data: forged })).status, 401);
    assert.equal((await deliver('header-mismatch', 'delayed', { id: 'different' })).status, 401);
    assert.equal(
      (
        await fetch(app.base + path, {
          method: 'POST',
          headers: { Cookie: app.cookie, 'Content-Type': 'application/json' },
          body: '{}',
        })
      ).status,
      403,
    );
    assert.equal((await deliver('before-activation')).status, 200);
    assert.equal(app.provider.requests.length, 0);
    const directory = (await app.request(path, app.cookie)).value;
    assert.equal(directory.choices.length, 1);
    assert.equal(directory.choices[0].chat, chat);
    assert.deepEqual(directory.choices[0].source.fields, ['order_id', 'status']);
    browser = await chromium.launch({ headless: true });
    async function pageFor(cookie) {
      const context = await browser.newContext({ viewport: { width: 1440, height: 960 } });
      const [name, value] = cookie.split('=');
      await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
      const page = await context.newPage();
      page.on('pageerror', (e) => errors.push(e.message));
      pages.push(page);
      return page;
    }
    const page = await pageFor(app.cookie),
      agents = `${app.base}/w/${w}/?view=agents`;
    await page.goto(agents);
    await page.getByRole('tab', { name: 'Automation', exact: true }).click();
    await page.getByRole('button', { name: 'Create event trigger', exact: true }).click();
    let dialog = page.getByRole('dialog', { name: 'Create event trigger', exact: true });
    const prompt = 'Ask delivery method then prepare a separately approved connected update';
    await dialog.getByRole('textbox', { name: 'Trigger name', exact: true }).fill('Review delayed orders');
    await dialog.getByRole('textbox', { name: 'Task instructions', exact: true }).fill(prompt);
    await dialog.getByRole('textbox', { name: 'status', exact: true }).fill('delayed');
    await dialog.getByRole('button', { name: 'Review event trigger', exact: true }).click();
    await dialog.getByText('orders@example.test', { exact: true }).waitFor();
    assert.equal(app.provider.requests.length, 0);
    await page.screenshot({ path: join(app.data, 'events-review-desktop.png') });
    await dialog.getByRole('button', { name: 'Activate event trigger', exact: true }).evaluate((button) => {
      button.click();
      button.click();
    });
    await dialog.getByText(/Event trigger saved/).waitFor();
    await dialog.getByRole('button', { name: 'Done', exact: true }).click();
    const triggers = (await app.request(path, app.cookie)).value.triggers;
    assert.equal(triggers.length, 1);
    const trigger = triggers[0];
    assert.equal((await deliver('before-activation')).status, 200);
    assert.equal((await deliver('unmatched', 'on-time')).status, 200);
    assert.equal(Number(app.sql('SELECT count(*) FROM relay_event_deliveries')), 0);
    const specification = {
      version: 1,
      kind: 'single_choice',
      title: 'Connected delivery method',
      options: [
        { id: 'road', label: 'Road' },
        { id: 'air', label: 'Air' },
      ],
    };
    const write = {
      tool: 'request_action',
      arguments: {
        connection,
        operation: 'order.update',
        arguments: { order: 'one', address: 'ONE_APPROVED_EVENT_UPDATE' },
      },
    };
    app.provider.setControl(prompt, {
      steps: [{ tool: 'ask_question', arguments: { key: 'delivery', specification } }],
      checkpoint: 'I am working on the event and waiting for your delivery choice.',
      afterQuestion: [{ tool: 'operations', arguments: {} }, write],
      questionCheckpoint: 'Your choice is saved. The proposed order update needs separate approval.',
      after: [{ tool: 'operations', arguments: {} }, write],
    });
    let executor = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
    const delivered = await Promise.all([deliver('event-one'), deliver('event-one')]);
    assert.ok(delivered.every((r) => r.status === 200));
    const duplicates = await Promise.all(delivered.map((r) => r.json()));
    assert.deepEqual(duplicates.map((x) => x.duplicate).sort(), [false, true]);
    assert.equal((await deliver('event-one', 'different')).status, 409);
    const deliveries = () =>
      JSON.parse(
        app.sql(
          `SELECT coalesce(json_agg(d ORDER BY checked_ms),'[]'::json) FROM relay_event_deliveries d WHERE trigger_id='${trigger.id}'`,
        ),
      );
    await until(() => deliveries().some((d) => d.reply), 'Event never admitted');
    const delivery = deliveries()[0];
    const job = JSON.parse(app.sql(`SELECT payload FROM relay_chat_jobs WHERE reply='${delivery.reply}'`));
    assert.equal(job.requester, owner.id);
    assert.equal(job.chat, chat);
    assert.ok(!job.prompt.includes('PRIVATE_EVENT_ORDER'));
    const state = () => app.sql(`SELECT state FROM relay_chat_jobs WHERE reply='${job.reply}'`);
    await until(() => state() === 'waiting', 'Event question did not release real Goose', 35000);
    assert.equal(remote.calls.length, 0);
    assert.ok(JSON.stringify(app.provider.requests).includes('PRIVATE_EVENT_ORDER'));
    assert.ok(!JSON.stringify(app.provider.requests).includes('NEVER_PROJECT_THIS_FIELD'));
    const question = JSON.parse(app.sql(`SELECT to_jsonb(q) FROM relay_task_questions q WHERE reply='${job.reply}'`));
    assert.equal((await app.request(`${app.path}/questions/${question.id}`, app.cookie)).value.source_scope, source.id);
    const calls = app.provider.requests.length;
    await app.request(`${path}/${trigger.id}/runs`, app.cookie);
    await app.request(`${app.path}/attention?view=needs_you`, app.cookie);
    assert.equal(app.provider.requests.length, calls);
    await app.request(`${path}/${trigger.id}/state`, app.cookie, {
      request: randomUUID(),
      revision: trigger.revision,
      enabled: false,
      connection_revision: null,
      read_revision: null,
      delegation_revision: null,
    });
    assert.equal(state(), 'waiting');
    await app.stop(executor);
    await app.stopApi();
    await app.startApi();
    executor = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
    assert.equal(state(), 'waiting');
    assert.equal(app.provider.requests.length, calls);
    assert.equal((await app.request(`${app.path}/questions/${question.id}`, member)).value.can_respond, false);
    await app.request(
      `${app.path}/questions/${question.id}`,
      member,
      { id: randomUUID(), revision: 1, decision: 'answer', values: { choices: ['road'] } },
      400,
    );
    await app.request(`${app.path}/questions/${question.id}`, app.cookie, {
      id: randomUUID(),
      revision: 1,
      decision: 'answer',
      values: { choices: ['road'] },
    });
    await until(
      () =>
        state() === 'waiting' &&
        Number(app.sql(`SELECT count(*) FROM relay_control_actions WHERE job='${job.reply}'`)) === 1,
      'Answered task did not request separate approval',
      35000,
    );
    const action = JSON.parse(app.sql(`SELECT to_jsonb(a) FROM relay_control_actions a WHERE job='${job.reply}'`));
    assert.equal(action.state, 'pending');
    assert.equal(remote.calls.length, 0);
    assert.equal((await app.request(`${app.path}/control/actions/${action.id}`, member)).value.can_decide, false);
    await app.request(
      `${app.path}/control/actions/${action.id}/decision`,
      member,
      { id: randomUUID(), decision: 'approve' },
      400,
    );
    await app.request(`${control}/actions/${action.id}/decision`, app.cookie, {
      id: randomUUID(),
      decision: 'approve',
    });
    await until(() => state() === 'finished', 'Approved event continuation did not finish', 35000);
    assert.equal(remote.calls.filter((c) => c.name === 'update_order').length, 1);
    assert.equal(remote.records.get('one').address, 'ONE_APPROVED_EVENT_UPDATE');
    assert.equal((await deliver('event-one')).status, 200);
    assert.equal(deliveries().length, 1);
    await page.goto(`${app.base}/w/${w}/?view=chat&conversation=${chat}&message=${job.reply}`);
    const responsePath = `${app.path}/chats/${chat}/messages/${job.reply}/response`;
    const result = (await app.request(responsePath, app.cookie)).value;
    assert.equal(result.source_scope, source.id);
    assert.match(result.body, /CONTINUED_RESULT/);
    const independent = (
      await app.request(`${app.path}/chats/${chat}/messages`, app.cookie, {
        id: randomUUID(),
        content: 'Independent follow-up request',
        agent: app.profile.id,
      })
    ).value;
    assert.equal(independent.ok, true);
    const later = JSON.parse(app.sql(`SELECT payload FROM relay_chat_jobs ORDER BY sequence DESC LIMIT 1`));
    assert.ok(!later.prompt.includes('CONTINUED_RESULT'));
    assert.ok(!later.prompt.includes('PRIVATE_EVENT_ORDER'));
    await app.request(`${app.path}/chats/${chat}/messages/${later.reply}/cancel`, app.cookie, {});
    await app.request(`${control}/rules`, app.cookie, {
      connection,
      operation: 'order.read',
      mode: 'block',
      revision: 1,
      chat,
    });
    await app.request(responsePath, app.cookie, undefined, 400);
    await app.request(`${app.path}/control/actions/${action.id}`, app.cookie, undefined, 400);
    await until(
      async () => !(await page.content()).includes('CONTINUED_RESULT'),
      'Event-derived content survived live source revocation',
    );
    const fresh = (await app.request(path, app.cookie)).value.triggers[0];
    assert.equal(fresh.state, 'blocked');
    await app.request(`${control}/rules`, app.cookie, {
      connection,
      operation: 'order.read',
      mode: 'allow',
      revision: 2,
      chat,
    });
    await app.request(responsePath, app.cookie, undefined, 400);
    await page.goto(agents);
    await page.getByRole('tab', { name: 'Automation', exact: true }).click();
    await page
      .getByRole('article', { name: 'Event trigger: Review delayed orders', exact: true })
      .getByRole('button', { name: 'Review trigger and deliveries', exact: true })
      .click();
    dialog = page.getByRole('dialog', { name: 'Review event trigger', exact: true });
    await dialog.getByRole('button', { name: 'Resume event trigger', exact: true }).waitFor();
    await page.screenshot({ path: join(app.data, 'events-deliveries-desktop.png') });
    await page.setViewportSize({ width: 390, height: 844 });
    await page.screenshot({ path: join(app.data, 'events-deliveries-phone.png') });
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.screenshot({ path: join(app.data, 'events-deliveries-phone-dark.png') });
    await app.stop(executor);
    const current = (await app.request(path, app.cookie)).value.triggers[0];
    await app.request(`${path}/${trigger.id}/state`, app.cookie, {
      request: randomUUID(),
      revision: current.revision,
      enabled: true,
      ...current.choice.authority,
    });
    assert.equal((await deliver('event-two')).status, 200);
    await until(
      () => deliveries().some((d) => d.event_id === 'event-two' && d.reply),
      'Fresh event was not admitted after explicit resume',
    );
    const second = deliveries().find((d) => d.event_id === 'event-two');
    assert.equal(app.sql(`SELECT state FROM relay_chat_jobs WHERE reply='${second.reply}'`), 'queued');
    await app.request(`${app.path}/chats/${chat}/messages/${second.reply}/cancel`, app.cookie, {});
    const stoppedCalls = app.provider.requests.length;
    assert.equal((await deliver('event-two')).status, 200);
    await app.stopApi();
    await app.startApi();
    assert.equal(app.sql(`SELECT state FROM relay_chat_jobs WHERE reply='${second.reply}'`), 'finished');
    assert.equal(deliveries().length, 2);
    assert.equal(app.provider.requests.length, stoppedCalls);
    await app.request(responsePath, app.cookie, undefined, 400);
    const finalCalls = app.provider.requests.length;
    await app.request(`${path}/${trigger.id}/runs`, app.cookie);
    assert.equal(app.provider.requests.length, finalCalls);
    assert.deepEqual(errors, []);
    app.pass(
      'Signed events, exact receipts, original-task source isolation, restart question and independent approved write, revocation/regrant, no status inference and responsive UI',
    );
    await writeFile(
      join(app.data, 'events-evidence.json'),
      JSON.stringify(
        {
          data: app.data,
          trigger: trigger.id,
          source: source.id,
          reply: job.reply,
          deliveries: deliveries().length,
          model_requests: app.provider.requests.length,
          confirmed_writes: remote.calls.filter((c) => c.name === 'update_order').length,
          errors,
        },
        null,
        2,
      ),
    );
  },
);
