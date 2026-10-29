import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID, createHmac } from 'node:crypto';
import { createRequire } from 'node:module';
import { mkdtemp, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { remoteControlMcp } from './fixtures/remote_control_mcp.mjs';
import { until, root } from './fixtures/polling.mjs';
const { chromium } = createRequire(join(root, 'web/package.json'))('@playwright/test');

test(
  'a helper event-read revocation clears an open parent reader and all derived handoff sources after real Goose has returned the result',
  {
    skip: !process.env.RELAY_TEST_DATABASE_URL,
    timeout: 150000,
  },
  async (t) => {
    let app, page, browser;
    const errors = [];
    t.after(async () => {
      try {
        if (app && page && !t.passed) {
          await page.screenshot({ path: join(app.data, 'handoff-source-failure.png'), timeout: 5000 }).catch(() => {});
          await writeFile(join(app.data, 'handoff-source-failure.html'), await page.content()).catch(() => {});
        }
      } finally {
        await browser?.close();
      }
    });
    const remote = await remoteControlMcp();
    t.after(() => remote.close());
    const dir = await mkdtemp(join(root, '.tmp/handoff-source-profile-')),
      profiles = join(dir, 'servers.json'),
      sources = join(dir, 'sources.json'),
      key = join(dir, 'signing.key'),
      bearer = join(dir, 'bearer'),
      secret = 'PRIVATE_HANDOFF_SOURCE_SIGNING_SECRET';
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
      helper = { ...app.profile, id: randomUUID(), name: 'Atlas', role: 'Review explicitly authorized source fields.' };
    await app.request(`${app.path}/agent-profiles`, app.cookie, helper);
    const channel = (
      await app.request(`${app.path}/chats`, app.cookie, {
        id: randomUUID(),
        kind: 'channel',
        name: 'Source protected handoff',
        users: [],
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
      await app.request(`${app.path}/agent-profiles/${agent}/control/rules`, app.cookie, {
        connection,
        operation: 'order.read',
        mode: 'allow',
        revision: 0,
        chat: channel,
      });
    const source = {
      id: randomUUID(),
      workspace: w,
      connection,
      connection_fingerprint: app.sql(`SELECT fingerprint FROM relay_control_connections WHERE id='${connection}'`),
      name: 'Reviewed delivery event',
      provider: 'composio_v3',
      user_id: 'source-user',
      connected_account_id: 'source-account',
      auth_config_id: 'source-auth',
      trigger_id: 'source-trigger',
      trigger_slug: 'ORDER_CHANGED',
      read_operation: 'order.read',
      fields: ['order_id', 'status'],
      secret_file: key,
    };
    await writeFile(sources, JSON.stringify([source]));
    const parentPrompt = 'Ask Atlas to review this authorized event source and report its saved result',
      childPrompt = 'PRIVATE_HANDOFF_REQUEST_TASK review the current authorized event fields';
    app.provider.setControl(parentPrompt, {
      steps: [{ tool: 'request_handoff', arguments: { key: 'event-helper', agent: helper.id, task: childPrompt } }],
      checkpoint: 'Atlas is reviewing the authorized event source.',
    });
    const homePanel = randomUUID();
    app.provider.setControl(childPrompt, {
      steps: [{tool:'save_home_panel',arguments:{id:homePanel,expected_revision:0,title:'PRIVATE_HANDOFF_PANEL_TITLE',purpose:'Source-derived saved view',sections:[{kind:'text',heading:'Source finding',body:'PRIVATE_HANDOFF_PANEL_BODY'}],sources:[]}}],
      checkpoint: 'PRIVATE_HELPER_SOURCE_RESULT ' + 'Saved source evidence. '.repeat(160),
    });
    const path = `${app.path}/agent-profiles/${app.profile.id}/event-triggers`,
      choice = (await app.request(path, app.cookie)).value.choices[0];
    assert.ok(choice);
    const trigger = {
      id: randomUUID(),
      source: source.id,
      binding: choice.source.binding,
      chat: channel,
      name: 'Event with governed helper',
      instructions: parentPrompt,
      filter: {},
      ...choice.authority,
    };
    await app.request(path, app.cookie, trigger);
    const executor = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
    const id = 'handoff-source-event',
      payload = {
        id,
        type: 'composio.trigger.message',
        metadata: {
          user_id: source.user_id,
          connected_account_id: source.connected_account_id,
          auth_config_id: source.auth_config_id,
          trigger_id: source.trigger_id,
          trigger_slug: source.trigger_slug,
        },
        data: { order_id: 'PRIVATE_ORIGINAL_EVENT_ORDER', status: 'delayed', excluded: 'NEVER_COPY_SOURCE_EXTRA' },
      };
    const raw = JSON.stringify(payload),
      time = String(Math.floor(Date.now() / 1000)),
      signature = createHmac('sha256', secret).update(`${id}.${time}.${raw}`).digest('base64');
    const delivered = await fetch(`${app.base}/hooks/composio/workspaces/${w}/${source.id}`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'webhook-id': id,
        'webhook-timestamp': time,
        'webhook-signature': `v1,${signature}`,
      },
      body: raw,
    });
    assert.equal(delivered.status, 200);
    await until(
      () => app.sql(`SELECT count(*) FROM relay_agent_handoffs WHERE principal IS NOT NULL`) === '1',
      'Event did not hand off to its source-authorized helper',
      35000,
    );
    const h = JSON.parse(app.sql('SELECT to_jsonb(h) FROM relay_agent_handoffs h'));
    const parent = JSON.parse(app.sql(`SELECT payload FROM relay_chat_jobs WHERE reply='${h.parent}'`)),
      child = JSON.parse(app.sql(`SELECT payload FROM relay_chat_jobs WHERE reply='${h.child}'`));
    await app.done(parent);
    await app.done(child);
    assert.equal(remote.calls.length, 0);
    assert.ok(JSON.stringify(app.provider.requests).includes('PRIVATE_ORIGINAL_EVENT_ORDER'));
    assert.ok(!JSON.stringify(app.provider.requests).includes('NEVER_COPY_SOURCE_EXTRA'));
    const responsePath = (task) => `${app.path}/chats/${channel}/messages/${task}/response`;
    const before = (await app.request(responsePath(parent.reply), app.cookie)).value;
    assert.equal(before.source_scope, source.id);
    assert.ok(before.body.includes('PRIVATE_HELPER_SOURCE_RESULT'));
    browser = await chromium.launch({ headless: true });
    const context = await browser.newContext({ viewport: { width: 1440, height: 960 } });
    const [name, value] = app.cookie.split('=');
    await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
    page = await context.newPage();
    page.on('pageerror', (e) => errors.push(e.message));
    await page.goto(`${app.base}/w/${w}/?view=chat&conversation=${channel}&message=${parent.reply}`);
    await page
      .locator(`[data-message-id="${parent.reply}"]`)
      .getByRole('button', { name: 'Expand response', exact: true })
      .click();
    await page.locator(`[data-message-id="${parent.reply}"]`).getByRole('button',{name:'Open response reader',exact:true}).click();
    const reader = page.getByRole('dialog', { name: 'Agent response', exact: true });
    await reader.locator('.result-body').filter({ hasText: 'PRIVATE_HELPER_SOURCE_RESULT' }).waitFor();
    await page.screenshot({ path: join(app.data, 'handoff-source-parent-reader-desktop.png') });
    const progressPath = task => `${app.path}/chats/${channel}/messages/${task}/progress`;
    const progressRecord = (await app.request(progressPath(parent.reply), app.cookie)).value;
    assert.equal(progressRecord.source_scope, source.id);
    const inlinePage = await context.newPage();
    inlinePage.on('pageerror', e => errors.push(e.message));
    await inlinePage.goto(`${app.base}/w/${w}/?view=chat&conversation=${channel}&message=${parent.reply}`);
    const inlineReply = inlinePage.locator(`[data-message-id="${parent.reply}"]`);
    assert.equal(await inlinePage.getByRole('button',{name:'Check progress',exact:true}).count(),0);
    await inlineReply.getByRole('button',{name:'Expand response',exact:true}).click();
    await inlineReply.locator('.inline-response-body').filter({hasText:'PRIVATE_HELPER_SOURCE_RESULT'}).waitFor();
    await inlinePage.screenshot({path:join(app.data,'handoff-source-inline-desktop.png')});
    await app.request(`${app.path}/home-panels/${homePanel}/review`,app.cookie,{request:randomUUID(),revision:1});
    const homePage = await context.newPage();
    homePage.on('pageerror',e=>errors.push(e.message));
    await homePage.goto(`${app.base}/w/${w}/?view=home`);
    await homePage.locator(`[data-panel="${homePanel}"]`).getByText('PRIVATE_HANDOFF_PANEL_BODY',{exact:true}).waitFor();
    await homePage.locator(`[data-panel="${homePanel}"]`).getByRole('button',{name:'Sources and details',exact:true}).click();
    await homePage.getByRole('dialog',{name:'Saved panel details',exact:true}).getByText('PRIVATE_HANDOFF_PANEL_BODY',{exact:true}).waitFor();
    await homePage.screenshot({path:join(app.data,'handoff-source-home-panel-desktop.png')});
    const calls = app.provider.requests.length;
    await app.request(`${app.path}/agent-profiles/${helper.id}/control/rules`, app.cookie, {
      connection,
      operation: 'order.read',
      mode: 'block',
      revision: 1,
      chat: channel,
    });
    await until(
      async () => !(await page.content()).includes('PRIVATE_HELPER_SOURCE_RESULT'),
      'Helper source leaked through the open parent reader after revocation',
      12000,
    );
    await reader.getByRole('button',{name:'Retry',exact:true}).waitFor();
    await until(()=>reader.getByRole('button',{name:'Retry',exact:true}).isEnabled(),'Denied reader left retry disabled');
    await reader.getByRole('button',{name:'Retry',exact:true}).click();
    assert.equal(await reader.locator('.result-body').count(),0);
    for (const task of [parent.reply, child.reply, h.request_message])
      await app.request(responsePath(task), app.cookie, undefined, 400);
    await until(async()=>!(await homePage.content()).includes('PRIVATE_HANDOFF_PANEL_BODY'),'Revoked helper source remained in saved Home panel',12000);
    assert.equal((await app.request(`${app.path}/home-panels`,app.cookie)).value.panels.find(p=>p.id===homePanel).active.available,false);
    await homePage.screenshot({path:join(app.data,'handoff-source-home-panel-revoked-desktop.png')});
    await until(async()=>!(await inlinePage.content()).includes('PRIVATE_HELPER_SOURCE_RESULT'),'Revoked source remained in expanded inline reply',12000);
    assert.equal(await inlineReply.locator('.inline-response-body').count(),0);
    for (const task of [parent.reply,child.reply]) await app.request(progressPath(task),app.cookie,undefined,400);
    await inlinePage.screenshot({path:join(app.data,'handoff-source-inline-revoked-desktop.png')});
    await page.screenshot({ path: join(app.data, 'handoff-source-revoked-reader-desktop.png') });
    assert.equal(app.provider.requests.length, calls);
    await app.request(`${app.path}/agent-profiles/${helper.id}/control/rules`, app.cookie, {
      connection,
      operation: 'order.read',
      mode: 'allow',
      revision: 2,
      chat: channel,
    });
    await app.request(responsePath(parent.reply), app.cookie, undefined, 400);
    await app.stop(executor);
    await app.stopApi();
    await app.startApi();
    await page.reload();
    assert.ok(!(await page.content()).includes('PRIVATE_HELPER_SOURCE_RESULT'));
    assert.equal(app.provider.requests.length, calls);
    assert.equal(remote.calls.length, 0);
    assert.deepEqual(errors, []);
    app.pass(
      'Real Goose event-to-helper-to-parent source provenance; independent receiver read; open parent reader invalidation; source request/child/parent denial after regrant and restart; zero business effects or status inference',
    );
    await writeFile(
      join(app.data, 'handoff-source-evidence.json'),
      JSON.stringify(
        {
          data: app.data,
          parent: parent.reply,
          child: child.reply,
          handoff: h.id,
          home_panel: homePanel,
          source: source.id,
          model_requests: app.provider.requests.length,
          mcp_calls: remote.calls.length,
          errors,
        },
        null,
        2,
      ),
    );
  },
);
