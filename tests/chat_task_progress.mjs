import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { writeFile } from 'node:fs/promises';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { until, root } from './fixtures/goose_app.mjs';
const { chromium } = createRequire(join(root, 'web/package.json'))('@playwright/test');
test(
  'Saved progress remains read-only while Chat keeps question/restart/Stop controls without progress launchers',
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
              .screenshot({ path: join(app.data, `progress-failure-${i}.png`), timeout: 5000 })
              .catch(() => {});
            await writeFile(join(app.data, `progress-failure-${i}.html`), await p.content()).catch(() => {});
          }
      } finally {
        await browser?.close();
      }
    });
    app = await workerFixture(t, {
      mode: 'pooled',
      control: true,
      capacity: 1,
      workspaceLimit: 1,
      modelLimit: 1,
      leaseSeconds: 15,
    });
    const w = app.path.split('/').at(-1),
      reader = await app.login(1),
      person = (await app.request('/api/identity/session', reader)).value.user;
    const chat = (
      await app.request(`${app.path}/chats`, app.cookie, {
        id: randomUUID(),
        kind: 'channel',
        name: 'Progress coordination',
        users: [person.id],
        agents: [app.profile.id],
      })
    ).value.id;
    const command = 'Prepare a quote after asking which delivery option to use.';
    const specification = {
      version: 1,
      kind: 'single_choice',
      title: 'Choose delivery for the quote',
      options: [
        { id: 'road', label: 'Road' },
        { id: 'air', label: 'Air' },
      ],
    };
    app.provider.setControl(command, {
      steps: [{ tool: 'ask_question', arguments: { key: 'delivery', specification } }],
      checkpoint: 'The draft is prepared. I need your delivery choice before continuing.',
    });
    const job = await app.send(command, chat);
    const path = `${app.path}/chats/${chat}/messages/${job.reply}/progress`,
      state = () =>
        JSON.parse(app.sql(`SELECT row_to_json(j) FROM relay_chat_jobs j WHERE reply='${job.reply}'`));
    const initial = (await app.request(path, reader)).value;
    assert.equal(initial.state, 'queued');
    assert.equal(initial.can_cancel, false);
    assert.equal(app.provider.requests.length, 0);
    await app.request(
      `${app.path}/agent-profiles/${app.profile.id}/requests/${job.reply}`,
      reader,
      undefined,
      400,
    );
    await app.request(
      `${app.path}/chats/${app.conversation}/messages/${job.reply}/progress`,
      reader,
      undefined,
      400,
    );
    const history = await app.history(chat);
    assert.equal(history.find((m) => m.id === job.reply).has_task, true);
    browser = await chromium.launch({ headless: true });
    async function pageFor(cookie) {
      const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
      const [name, value] = cookie.split('=');
      await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
      const page = await context.newPage();
      page.on('pageerror', (e) => errors.push(e.message));
      pages.push(page);
      return page;
    }
    const page = await pageFor(app.cookie),
      observer = await pageFor(reader),
      url = `${app.base}/w/${w}/?view=chat&conversation=${chat}&message=${job.reply}`;
    await page.goto(url);
    const message = page.locator(`[data-message-id="${job.reply}"]`);
    await message.waitFor();
    assert.equal(await page.getByRole('button', { name: 'Check progress', exact: true }).count(), 0);
    await app.request(path, reader);
    assert.equal(app.provider.requests.length, 0);
    app.pass(
      'Actual task flags and source-bound GET permit a channel reader without request permission, deny the wrong conversation and perform no model work',
    );
    const worker = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
    await until(() => state().state === 'waiting', 'Question did not release its worker', 35000);
    const count = app.provider.requests.length,
      before = state();
    for (let i = 0; i < 5; i++) {
      const current = (await app.request(path, reader)).value;
      assert.equal(current.state, 'input');
      assert.equal(current.worker_released, true);
      assert.equal(current.can_cancel, false);
      assert.equal(current.questions[0].respondent, before.payload.requester);
    }
    assert.deepEqual(state(), before);
    assert.equal(app.provider.requests.length, count);
    await message.getByRole('button', { name: 'Answer question', exact: true }).waitFor();
    await page.screenshot({ path: join(app.data, 'chat-question-desktop.png') });
    await observer.goto(url);
    const observed = observer.locator(`[data-message-id="${job.reply}"]`);
    await observed.getByRole('button', { name: 'View question', exact: true }).waitFor();
    assert.equal(await observed.getByRole('button', { name: 'Stop reply', exact: true }).count(), 0);
    assert.equal(await observer.getByRole('button', { name: 'Check progress', exact: true }).count(), 0);
    await observer.setViewportSize({ width: 390, height: 844 });
    await observer.screenshot({ path: join(app.data, 'chat-observer-phone.png') });
    assert.equal(await observer.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
    await app.stop(worker);
    await app.stopApi();
    await app.startApi();
    assert.equal((await app.request(path, reader)).value.state, 'input');
    assert.equal(app.provider.requests.length, count);
    await message.getByRole('button', { name: 'Answer question', exact: true }).click();
    const answer = page.getByRole('dialog', { name: specification.title, exact: true });
    await answer.getByRole('radio', { name: 'Road', exact: true }).check();
    await answer.getByRole('button', { name: 'Submit answer', exact: true }).click();
    await answer.getByText('Answer saved', { exact: true }).waitFor();
    await answer.getByRole('button', { name: 'Close', exact: true }).click();
    await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
    await app.done(job);
    const completed = (await app.request(path, reader)).value;
    assert.equal(completed.state, 'complete');
    assert.equal(completed.can_cancel, false);
    await page.setViewportSize({ width: 390, height: 844 });
    await page.evaluate(() => {
      document.documentElement.dataset.theme = 'dark';
    });
    await page.screenshot({ path: join(app.data, 'task-progress-complete-phone-dark.png') });
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
    const finishedCount = app.provider.requests.length;
    for (let i = 0; i < 3; i++) await app.request(path, reader);
    assert.equal(app.provider.requests.length, finishedCount);
    app.pass(
      'Saved input progress releases execution, repeated reads preserve the exact job, controller restart retains the wait and Chat question review continues the same task to completion',
    );
    const stopped = await app.send(command, chat);
    await until(
      () => app.sql(`SELECT state FROM relay_chat_jobs WHERE reply='${stopped.reply}'`) === 'waiting',
      'Second question did not wait',
      35000,
    );
    await page.setViewportSize({ width: 1280, height: 900 });
    await page.goto(`${app.base}/w/${w}/?view=chat&conversation=${chat}&message=${stopped.reply}`);
    await page.locator(`[data-message-id="${stopped.reply}"]`).getByRole('button', { name: 'Stop reply', exact: true }).click();
    await app.done(stopped, 'cancelled');
    const cancelled = (await app.request(`${app.path}/chats/${chat}/messages/${stopped.reply}/progress`, reader)).value;
    assert.equal(cancelled.state, 'cancelled'); assert.equal(cancelled.can_cancel, false);
    await page.screenshot({ path: join(app.data, 'task-progress-stopped-desktop.png') });
    assert.deepEqual(errors, []);
    app.pass(
      'The original person stops exactly this saved task; read-only participants receive no Stop authority and small/dark Chat views remain bounded',
    );
    await writeFile(
      join(app.data, 'task-progress-evidence.json'),
      JSON.stringify(
        {
          data: app.data,
          reply: job.reply,
          stopped: stopped.reply,
          model_requests: app.provider.requests.length,
          errors,
        },
        null,
        2,
      ),
    );
  },
);
