import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { mkdtemp, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { remoteControlMcp } from './fixtures/remote_control_mcp.mjs';
import { until, root } from './fixtures/goose_app.mjs';
const { chromium } = createRequire(join(root, 'web/package.json'))('@playwright/test');

test(
  'real Goose channel handoffs release the sender, survive restart, retain questions and independent approvals and stop the exact linked work',
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
            await p.screenshot({ path: join(app.data, `handoff-failure-${i}.png`), timeout: 5000 }).catch(() => {});
            await writeFile(join(app.data, `handoff-failure-${i}.html`), await p.content()).catch(() => {});
          }
      } finally {
        await browser?.close();
      }
    });
    const remote = await remoteControlMcp();
    t.after(() => remote.close());
    const dir = await mkdtemp(join(root, '.tmp/handoff-profile-')),
      profiles = join(dir, 'servers.json'),
      bearer = join(dir, 'bearer');
    await writeFile(profiles, '[]', { mode: 0o600 });
    await writeFile(bearer, remote.token, { mode: 0o600 });
    app = await workerFixture(t, {
      mode: 'pooled',
      control: true,
      capacity: 1,
      workspaceLimit: 1,
      modelLimit: 1,
      leaseSeconds: 15,
      apiEnv: { AYATI_MCP_SERVERS_FILE: profiles },
    });
    const w = app.path.split('/').at(-1),
      owner = (await app.request('/api/identity/session', app.cookie)).value.user;
    const helper = {
      ...app.profile,
      id: randomUUID(),
      name: 'Atlas',
      role: 'Review orders and ask before changing them.',
    };
    await app.request(`${app.path}/agent-profiles`, app.cookie, helper);
    const member = await app.login(1),
      person = (await app.request('/api/identity/session', member)).value.user;
    for (const agent of [app.profile.id, helper.id])
      await app.request(`${app.path}/chat-grants`, app.cookie, { user: person.id, agent, allowed: true });
    const channel = (
      await app.request(`${app.path}/chats`, app.cookie, {
        id: randomUUID(),
        kind: 'channel',
        name: 'Channel handoff operations',
        users: [person.id],
        agents: [app.profile.id, helper.id],
      })
    ).value.id;
    const connection = randomUUID();
    await writeFile(
      profiles,
      JSON.stringify([
        {
          id: 'orders',
          workspace: w,
          name: 'Orders',
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
    for (const agent of [app.profile.id, helper.id])
      for (const [operation, mode] of [
        ['order.read', 'allow'],
        ['order.update', 'ask'],
      ])
        await app.request(`${app.path}/agent-profiles/${agent}/control/rules`, app.cookie, {
          connection,
          operation,
          mode,
          revision: 0,
          chat: channel,
        });
    const parentPrompt = 'Delegate the order review to Atlas then summarize the saved outcome',
      childPrompt = 'Ask delivery method then prepare the separately approved helper update';
    const ask = {
      version: 1,
      kind: 'single_choice',
      title: 'Helper delivery method',
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
        arguments: { order: 'one', address: 'ONE_APPROVED_HANDOFF_UPDATE' },
      },
    };
    app.provider.setControl(parentPrompt, {
      steps: [
        { tool: 'handoff_targets', arguments: {} },
        { tool: 'request_handoff', arguments: { key: 'review-order', agent: helper.id, task: childPrompt } },
      ],
      checkpoint: 'Atlas is reviewing the order. I am waiting for the saved helper result.',
    });
    app.provider.setControl(childPrompt, {
      steps: [{ tool: 'ask_question', arguments: { key: 'delivery', specification: ask } }],
      checkpoint: 'I need a delivery choice before preparing the update.',
      afterQuestion: [{ tool: 'operations', arguments: {} }, write],
      questionCheckpoint: 'The exact helper update needs a separate human approval.',
      after: [
        { tool: 'operations', arguments: {} },
        write,
        { tool: 'request_action', arguments: { connection, operation: 'order.read', arguments: { order: 'one' } } },
      ],
    });
    let executor = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
    const parent = await app.send(parentPrompt, channel);
    const handoffs = (task) =>
      JSON.parse(
        app.sql(
          `SELECT coalesce(json_agg(h ORDER BY created_ms),'[]'::json) FROM relay_agent_handoffs h WHERE parent='${task.reply}'`,
        ),
      );
    const state = (task) => app.sql(`SELECT state FROM relay_chat_jobs WHERE reply='${task.reply}'`);
    await until(() => handoffs(parent).length === 1, 'Root did not save a channel handoff', 35000);
    const h = handoffs(parent)[0],
      child = JSON.parse(app.sql(`SELECT payload FROM relay_chat_jobs WHERE reply='${h.child}'`));
    await until(
      () => state(parent) === 'waiting' && state(child) === 'waiting',
      'Sender/helper did not release while waiting',
      35000,
    );
    assert.equal(child.requester, owner.id);
    assert.equal(child.chat, channel);
    assert.equal(child.agent, helper.id);
    assert.equal(remote.calls.length, 0);
    assert.equal(
      app.sql(`SELECT kind||':'||author FROM relay_chat_messages WHERE id='${h.request_message}'`),
      `agent:${app.profile.id}`,
    );
    await until(
      () => app.sql("SELECT count(*) FROM relay_executor_slots WHERE state='ready'") === '1',
      'Handoff retained a worker',
      35000,
    );
    const idle = app.provider.requests.length;
    await app.request(`${app.path}/agent-profiles/${app.profile.id}/requests`, app.cookie);
    await app.request(`${app.path}/agent-profiles/${helper.id}/requests`, app.cookie);
    assert.equal(app.provider.requests.length, idle);
    browser = await chromium.launch({ headless: true });
    const context = await browser.newContext({ viewport: { width: 1440, height: 960 } });
    const [name, value] = app.cookie.split('=');
    await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
    const page = await context.newPage();
    pages.push(page);
    page.on('pageerror', (e) => errors.push(e.message));
    const url = (task) => `${app.base}/w/${w}/?view=chat&conversation=${channel}&message=${task.reply}`;
    await page.goto(url(parent));
    const card = page.getByRole('region', { name: 'Handoff to Atlas', exact: true });
    await card.getByText('Agent handoff · Waiting for an answer', { exact: true }).waitFor();
    await page.screenshot({ path: join(app.data, 'handoff-waiting-desktop.png') });
    await card.getByRole('link', { name: 'Open helper task', exact: true }).click();
    await page.getByRole('button', { name: 'Answer question', exact: true }).waitFor();
    const question = JSON.parse(app.sql(`SELECT to_jsonb(q) FROM relay_task_questions q WHERE reply='${child.reply}'`));
    assert.equal((await app.request(`${app.path}/questions/${question.id}`, member)).value.can_respond, false);
    await app.stop(executor);
    await app.stopApi();
    await app.startApi();
    executor = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
    await page.reload();
    assert.equal(state(parent), 'waiting');
    assert.equal(state(child), 'waiting');
    assert.equal(app.provider.requests.length, idle);
    await page.getByRole('button', { name: 'Answer question', exact: true }).click();
    let dialog = page.getByRole('dialog', { name: ask.title, exact: true });
    await dialog.getByLabel('Road', { exact: true }).check();
    await dialog.getByRole('button', { name: 'Submit answer', exact: true }).click();
    await dialog.getByText('Answer saved', { exact: true }).waitFor();
    await dialog.getByRole('button', { name: 'Close', exact: true }).click();
    await until(
      () =>
        app.sql(`SELECT count(*) FROM relay_control_actions WHERE job='${child.reply}'`) === '1' &&
        state(child) === 'waiting',
      'Helper did not request independent exact approval',
      35000,
    );
    assert.equal(state(parent), 'waiting');
    assert.equal(remote.calls.length, 0);
    await page.getByRole('button', { name: 'Review saved request', exact: true }).click();
    dialog = page.getByRole('dialog', { name: 'Review approval request', exact: true });
    await dialog.getByText('ONE_APPROVED_HANDOFF_UPDATE', { exact: true }).waitFor();
    await page.setViewportSize({ width: 390, height: 844 });
    await page.screenshot({ path: join(app.data, 'handoff-approval-phone.png') });
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
    await dialog.getByRole('button', { name: 'Approve this action', exact: true }).click();
    await dialog.getByRole('button', { name: 'Close', exact: true }).click();
    await app.done(child);
    const complete = await app.done(parent);
    assert.match(complete.content, /"handoffs"/);
    assert.match(complete.content, /"state":"complete"/);
    assert.equal(remote.calls.filter((c) => c.name === 'update_order').length, 1);
    assert.equal(remote.records.get('one').address, 'ONE_APPROVED_HANDOFF_UPDATE');
    await page.setViewportSize({ width: 1440, height: 960 });
    await page.goto(url(parent));
    await page
      .getByRole('region', { name: 'Handoff to Atlas', exact: true })
      .getByText('Agent handoff · Completed', { exact: true })
      .waitFor();
    await page.screenshot({ path: join(app.data, 'handoff-complete-desktop.png') });
    await page.setViewportSize({ width: 390, height: 844 });
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.screenshot({ path: join(app.data, 'handoff-complete-phone-dark.png') });
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
    const stopPrompt = 'Delegate a supplier question to Atlas and wait for its saved outcome',
      stopTask = 'Ask for the missing supplier fact and wait';
    app.provider.setControl(stopPrompt, {
      steps: [{ tool: 'request_handoff', arguments: { key: 'supplier', agent: helper.id, task: stopTask } }],
      checkpoint: 'Atlas is waiting for a supplier fact.',
    });
    app.provider.setControl(stopTask, {
      steps: [
        {
          tool: 'ask_question',
          arguments: { key: 'supplier', specification: { ...ask, title: 'Supplier fact for helper' } },
        },
      ],
      checkpoint: 'I am waiting for the supplier fact.',
    });
    const stopChildParent = await app.send(stopPrompt, channel);
    await until(() => handoffs(stopChildParent).length === 1, 'Second handoff missing', 35000);
    let stopped = handoffs(stopChildParent)[0];
    let stoppedTask = { reply: stopped.child, chat: channel };
    await until(
      () => state(stopChildParent) === 'waiting' && state(stoppedTask) === 'waiting',
      'Second helper did not wait',
      35000,
    );
    await page.goto(url(stopChildParent));
    await page
      .getByRole('region', { name: 'Handoff to Atlas', exact: true })
      .getByRole('button', { name: 'Stop helper task', exact: true })
      .click();
    await app.done(stopChildParent);
    assert.equal(state(stoppedTask), 'finished');
    assert.equal(handoffs(stopChildParent)[0].state, 'cancelled');
    const stopRoot = await app.send(stopPrompt, channel);
    await until(() => handoffs(stopRoot).length === 1, 'Third handoff missing', 35000);
    stopped = handoffs(stopRoot)[0];
    stoppedTask = { reply: stopped.child, chat: channel };
    await until(
      () => state(stopRoot) === 'waiting' && state(stoppedTask) === 'waiting',
      'Third helper did not wait',
      35000,
    );
    await app.request(`${app.path}/chats/${channel}/messages/${stopRoot.reply}/cancel`, app.cookie, {});
    assert.equal(state(stopRoot), 'finished');
    assert.equal(state(stoppedTask), 'finished');
    assert.equal(handoffs(stopRoot)[0].state, 'cancelled');
    const finalCalls = app.provider.requests.length;
    await app.stop(executor);
    await app.stopApi();
    await app.startApi();
    assert.equal(state(stoppedTask), 'finished');
    assert.equal(app.provider.requests.length, finalCalls);
    assert.equal(remote.calls.filter((c) => c.name === 'update_order').length, 1);
    assert.deepEqual(errors, []);
    app.pass(
      'Real Goose handoff/checkpoint release, exact question and independent approved helper write, original parent continuation, child/root Stop, restart and bounded desktop/phone/dark UI',
    );
    await writeFile(
      join(app.data, 'handoff-evidence.json'),
      JSON.stringify(
        {
          data: app.data,
          parent: parent.reply,
          child: child.reply,
          handoff: h.id,
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
