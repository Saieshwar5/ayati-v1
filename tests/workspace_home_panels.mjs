import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { mkdtemp, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { remoteControlMcp } from './fixtures/remote_control_mcp.mjs';
import { until, root } from './fixtures/polling.mjs';
const { chromium } = createRequire(join(root, 'web/package.json'))('@playwright/test');
test(
  'managed agents save personal Home drafts, exact review and second-device layout survive restart while source revocation hides retained snapshots',
  {
    skip: !process.env.RELAY_TEST_DATABASE_URL,
    timeout: 180000,
  },
  async (t) => {
    let app, browser;
    const pages = [],
      errors = [];
    t.after(async () => {
      try {
        if (app && !t.passed)
          for (const [i, p] of pages.entries()) {
            await p
              .screenshot({ path: join(app.data, `home-panel-failure-${i}.png`), timeout: 5000 })
              .catch(() => {});
            await writeFile(join(app.data, `home-panel-failure-${i}.html`), await p.content()).catch(
              () => {},
            );
          }
      } finally {
        await browser?.close();
      }
    });
    const remote = await remoteControlMcp();
    t.after(() => remote.close());
    const dir = await mkdtemp(join(root, '.tmp/home-panel-profile-')),
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
      member = await app.login(1),
      person = (await app.request('/api/identity/session', member)).value.user;
    await app.request(`${app.path}/chat-grants`, app.cookie, {
      user: person.id,
      agent: app.profile.id,
      allowed: true,
    });
    const channel = (
      await app.request(`${app.path}/chats`, app.cookie, {
        id: randomUUID(),
        kind: 'channel',
        name: 'Personal panel coordination',
        users: [person.id],
        agents: [app.profile.id],
      })
    ).value.id;
    const connection = randomUUID(),
      control = `${app.path}/agent-profiles/${app.profile.id}/control`;
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
    await app.request(`${app.path}/control/systems`, app.cookie, {
      server: 'orders',
      approved: true,
      revision: 0,
    });
    await app.request(`${app.path}/control/connections`, app.cookie, {
      id: connection,
      server: 'orders',
      enabled: true,
      revision: 0,
    });
    await app.request(`${control}/rules`, app.cookie, {
      connection,
      operation: 'order.read',
      mode: 'allow',
      revision: 0,
      chat: channel,
    });
    const first = randomUUID(),
      second = randomUUID(),
      command = 'Prepare my personal sample-order panel.';
    const draft = {
      id: first,
      expected_revision: 0,
      title: 'Sample order follow-up',
      purpose: 'Review the synthetic order basis',
      template: 'metric',
      sections: [{ kind: 'metric', label: 'Sample order', value: 'one' }],
      sources: [],
    };
    app.provider.setControl(command, {
      steps: [
        { tool: 'operations', arguments: {} },
        {
          tool: 'request_action',
          arguments: { connection, operation: 'order.read', arguments: { order: 'one' } },
        },
        { tool: 'save_home_panel', arguments: draft },
      ],
      checkpoint: 'I saved your personal Home draft. Review its basis on Home and choose Keep on Home.',
    });
    const executor = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
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
    const page = await pageFor(member),
      otherDevice = await pageFor(member),
      ownerPage = await pageFor(app.cookie),
      home = `${app.base}/w/${w}/?view=home`;
    await page.goto(home);
    await page.getByRole('button', { name: 'Ask for a panel', exact: true }).click();
    let dialog = page.getByRole('dialog', { name: 'Ask for a Home panel', exact: true });
    await dialog.getByLabel('Agent', { exact: true }).selectOption(app.profile.id);
    await dialog.getByLabel('Conversation', { exact: true }).selectOption(channel);
    await dialog.getByRole('textbox', { name: 'What should the panel show?', exact: true }).fill(command);
    const before = app.provider.requests.length;
    assert.equal(before, 0);
    await dialog.getByRole('button', { name: 'Ask agent', exact: true }).click();
    await until(
      () => app.sql(`SELECT count(*) FROM relay_home_panels WHERE id='${first}'`) === '1',
      'Real Goose did not save its personal draft',
      35000,
    );
    const firstJob = JSON.parse(
      app.sql(
        `SELECT j.payload FROM relay_home_panel_versions v JOIN relay_chat_jobs j ON j.workspace=v.workspace AND j.reply=v.task WHERE v.panel='${first}'`,
      ),
    );
    await app.done(firstJob);
    const read = () => app.request(`${app.path}/home-panels`, member);
    let saved = (await read()).value;
    assert.equal(saved.panels[0].active, null);
    assert.equal(saved.panels[0].draft.source_operations, 1);
    assert.equal((await app.request(`${app.path}/home-panels`, app.cookie)).value.panels.length, 0);
    await app.request(
      `${app.path}/home-panels/${first}/review`,
      app.cookie,
      { request: randomUUID(), revision: 1 },
      400,
    );
    await page.goto(home);
    let card = page.locator(`[data-panel="${first}"]`);
    await card.getByRole('button', { name: 'Review draft 1', exact: true }).click();
    dialog = page.getByRole('dialog', { name: 'Review Home panel', exact: true });
    await dialog.getByRole('button', { name: 'Keep on Home', exact: true }).click();
    await dialog.waitFor({ state: 'detached' });
    assert.equal((await read()).value.panels[0].active_revision, 1);
    await page
      .locator(`[data-panel="${first}"]`)
      .getByRole('button', { name: 'Sources and details', exact: true })
      .click();
    const details = page.getByRole('dialog', { name: 'Saved panel details', exact: true });
    await details.getByText('one', { exact: true }).waitFor();
    assert.equal(await details.getByRole('button', { name: 'Keep on Home', exact: true }).count(), 0);
    await page.keyboard.press('Escape');
    await details.waitFor({ state: 'detached' });
    const quietCount = app.provider.requests.length;
    await page.reload();
    await page.locator(`[data-panel="${first}"]`).waitFor();
    await otherDevice.goto(home);
    await otherDevice.locator(`[data-panel="${first}"]`).waitFor();
    for (let i = 0; i < 4; i++) await read();
    assert.equal(app.provider.requests.length, quietCount);
    assert.equal(remote.calls.length, 1);
    app.pass(
      'Explicit Home request uses real Goose and one authenticated source read, saves a personal draft and requires exact review; another Owner cannot view or publish it and Home reads add no inference',
    );
    const noteText = 'Prepare a personal priorities brief.';
    const note = {
      id: second,
      expected_revision: 0,
      title: 'My priorities',
      purpose: 'A personal finding',
      template: 'brief',
      sections: [
        {
          kind: 'text',
          heading: 'Next follow-up',
          body: 'KEEP_UNRELATED_PANEL <img src=x onerror=alert(1)>',
        },
      ],
      sources: [],
    };
    app.provider.setControl(noteText, {
      steps: [{ tool: 'save_home_panel', arguments: note }],
      checkpoint: 'Your priorities draft is ready for personal review.',
    });
    const noteJob = await app.send(noteText, channel, member);
    await app.done(noteJob);
    await page.getByRole('button', { name: 'Review draft 1', exact: true }).click();
    dialog = page.getByRole('dialog', { name: 'Review Home panel', exact: true });
    await dialog.getByRole('button', { name: 'Keep on Home', exact: true }).click();
    await dialog.waitFor({ state: 'detached' });
    const noteCard = page.locator(`[data-panel="${second}"]`);
    assert.equal(await noteCard.locator('img').count(), 0);
    await page.getByRole('button', { name: 'Arrange', exact: true }).click();
    card = page.locator(`[data-panel="${first}"]`);
    await card.getByRole('button', { name: 'Move later', exact: true }).click();
    await until(async () => (await read()).value.layout.order[0] === second, 'Personal order was not saved');
    await until(
      async () =>
        (await otherDevice.locator('.workspace-panels [data-panel]').first().getAttribute('data-panel')) ===
        second,
      'Second device did not receive personal order',
      12000,
    );
    await page
      .locator(`[data-panel="${second}"]`)
      .getByRole('button', { name: 'Hide from my Home', exact: true })
      .click();
    await until(
      async () => (await otherDevice.locator(`[data-panel="${second}"]`).count()) === 0,
      'Second device retained hidden placement',
      12000,
    );
    await otherDevice.getByRole('button', { name: 'Hidden panels', exact: true }).click();
    await otherDevice.getByRole('button', { name: 'Arrange', exact: true }).click();
    await otherDevice
      .locator(`[data-panel="${second}"]`)
      .getByRole('button', { name: 'Show on my Home', exact: true })
      .click();
    await otherDevice.getByRole('button', { name: 'Visible panels', exact: true }).click();
    await page.locator(`[data-panel="${second}"]`).waitFor();
    await page.getByRole('button', { name: 'Done arranging', exact: true }).click();
    const update = 'Revise the presentation of my sample-order panel.';
    const changed = {
      ...draft,
      expected_revision: 1,
      sections: [{ kind: 'metric', label: 'Reviewed sample', value: 'two' }],
    };
    app.provider.setControl(update, {
      steps: [
        { tool: 'personal_panels', arguments: {} },
        { tool: 'save_home_panel', arguments: changed },
      ],
      checkpoint: 'A new presentation is saved for review; the previously approved snapshot stays on Home.',
    });
    const beforeChange = app.provider.requests.length;
    await page
      .locator(`[data-panel="${first}"]`)
      .getByRole('button', { name: 'Ask for changes', exact: true })
      .click();
    dialog = page.getByRole('dialog', { name: 'Ask for changes', exact: true });
    await dialog.getByLabel('Agent', { exact: true }).waitFor();
    assert.equal(await dialog.getByLabel('Agent', { exact: true }).isDisabled(), true);
    await until(
      async () => (await dialog.getByLabel('Conversation', { exact: true }).inputValue()) === channel,
      'Original panel conversation was not loaded',
    );
    assert.equal(await dialog.getByLabel('Conversation', { exact: true }).inputValue(), channel);
    assert.equal(app.provider.requests.length, beforeChange);
    await dialog.getByRole('textbox', { name: 'What should the panel show?', exact: true }).fill(update);
    await page.setViewportSize({ width: 390, height: 844 });
    await page.screenshot({ path: join(app.data, 'workspace-panels-ask-change-phone.png') });
    await page.setViewportSize({ width: 1440, height: 960 });
    await dialog.getByRole('button', { name: 'Ask agent', exact: true }).click();
    await until(
      () =>
        app.sql(`SELECT count(*) FROM relay_home_panel_versions WHERE panel='${first}' AND revision=2`) ===
        '1',
      'Explicit panel change did not save its next draft',
      35000,
    );
    const revised = JSON.parse(
      app.sql(
        `SELECT j.payload FROM relay_home_panel_versions v JOIN relay_chat_jobs j ON j.workspace=v.workspace AND j.reply=v.task WHERE v.panel='${first}' AND v.revision=2`,
      ),
    );
    await app.done(revised);
    await page.goto(home);
    card = page.locator(`[data-panel="${first}"]`);
    await card.getByRole('button', { name: 'Review draft 2', exact: true }).waitFor();
    await card.getByText('one', { exact: true }).waitFor();
    assert.equal(
      await card.evaluate(
        (node) =>
          node.querySelector('.panel-metric-value').getBoundingClientRect().bottom <=
          node.querySelector('.panel-snapshot').getBoundingClientRect().bottom + 1,
      ),
      true,
    );
    assert.equal(await card.getByText('two', { exact: true }).count(), 0);
    assert.equal(
      await card.evaluate((node) => {
        const button = [...node.querySelectorAll('button')].find((b) => b.textContent === 'Review draft 2');
        return (
          button.getBoundingClientRect().bottom <= node.getBoundingClientRect().bottom && node.scrollTop === 0
        );
      }),
      true,
    );
    await page.screenshot({ path: join(app.data, 'workspace-panels-approved-and-draft-desktop.png') });
    await card.getByRole('button', { name: 'Review draft 2', exact: true }).click();
    dialog = page.getByRole('dialog', { name: 'Review Home panel', exact: true });
    await dialog.getByText('two', { exact: true }).waitFor();
    await page.setViewportSize({ width: 390, height: 844 });
    await page.screenshot({ path: join(app.data, 'workspace-panels-review-phone.png') });
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
    await dialog.getByRole('button', { name: 'Keep on Home', exact: true }).click();
    await dialog.waitFor({ state: 'detached' });
    await card.getByText('two', { exact: true }).waitFor();
    app.pass(
      'Personal order/hide/restore sync across two devices, agent markup remains inert and a later draft retains the approved snapshot until a new exact human review',
    );
    const count = app.provider.requests.length;
    await app.stop(executor);
    await app.stopApi();
    await app.startApi();
    await page.reload();
    await page.locator(`[data-panel="${first}"]`).getByText('two', { exact: true }).waitFor();
    assert.equal((await read()).value.layout.order[0], second);
    assert.equal(app.provider.requests.length, count);
    await page.evaluate(() => {
      document.documentElement.dataset.theme = 'dark';
    });
    await page.locator(`[data-panel="${first}"]`).scrollIntoViewIfNeeded();
    await page.screenshot({ path: join(app.data, 'workspace-panels-saved-phone-dark.png') });
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
    await app.request(`${control}/rules`, app.cookie, {
      connection,
      operation: 'order.read',
      mode: 'block',
      revision: 1,
      chat: channel,
    });
    await until(
      async () => !(await page.content()).includes('Sample order follow-up'),
      'Revoked operation left private cached panel visible',
      12000,
    );
    // Access invalidation first clears cached panels, then reloads the denied
    // projection. Wait for that projection rather than racing its empty state.
    await page.locator(`[data-panel="${first}"]`)
      .getByRole('heading', { name: 'Panel source unavailable', exact: true }).waitFor();
    await page.getByText('KEEP_UNRELATED_PANEL <img src=x onerror=alert(1)>', { exact: true }).waitFor();
    await page.setViewportSize({ width: 1440, height: 960 });
    await page.screenshot({ path: join(app.data, 'workspace-panels-source-revoked-desktop.png') });
    await app.request(`${control}/rules`, app.cookie, {
      connection,
      operation: 'order.read',
      mode: 'allow',
      revision: 2,
      chat: channel,
    });
    saved = (await read()).value;
    assert.equal(saved.panels.find((p) => p.id === first).active.available, false);
    assert.equal(saved.panels.find((p) => p.id === second).active.available, true);
    await ownerPage.goto(home);
    await ownerPage.getByRole('heading', { name: 'Your workspace panels', exact: true }).waitFor();
    assert.equal(await ownerPage.locator('.workspace-panels [data-panel]').count(), 0);
    assert.equal(remote.calls.length, 1);
    assert.equal(app.provider.requests.length, count);
    assert.deepEqual(errors, []);
    app.pass(
      'Controller restart preserves approved snapshots and layout without inference; source revocation clears the affected view, retains the unrelated personal panel and regrant cannot revive old receipts',
    );
    await writeFile(
      join(app.data, 'workspace-panels-evidence.json'),
      JSON.stringify(
        {
          data: app.data,
          panels: [first, second],
          model_requests: count,
          mcp_calls: remote.calls.length,
          confirmed_writes: remote.calls.filter((c) => c.name === 'update_order').length,
          errors,
        },
        null,
        2,
      ),
    );
  },
);
