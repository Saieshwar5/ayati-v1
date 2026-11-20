// Chat's first visit in a real browser against a real controller: an admin alone
// in a new workspace is offered their assistant, Invite people and Create agent;
// a ＋ with nobody to add offers the same instead of a form; each tile leads where
// it says; a member never sees Invite.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { root } from './fixtures/polling.mjs';
const { chromium } = createRequire(join(root, 'web/package.json'))('@playwright/test');

test('the first visit to Chat leads to your assistant, inviting people and creating an agent', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 300000,
}, async t => {
  let app, browser, page;
  t.after(async () => {
    try { if (app && page && !t.passed) await page.screenshot({ path: join(app.data, 'chat-first-visit-failure.png') }).catch(() => {}); }
    finally { await browser?.close(); }
  });
  app = await workerFixture(t, {});
  // A new workspace of its own: its creator is the only person in it.
  const alone = (await app.request('/api/identity/workspaces', app.cookie, { name: 'Northwind', request_id: randomUUID() })).value.id;
  browser = await chromium.launch({ headless: true });
  const visit = async (cookie, workspace) => {
    const context = await browser.newContext({ viewport: { width: 1280, height: 800 } });
    const [name, value] = cookie.split('=');
    await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
    page = await context.newPage();
    await page.goto(`${app.base}/w/${workspace}/?view=chat`);
    await page.getByRole('region', { name: 'Start' }).waitFor();
    return page;
  };

  page = await visit(app.cookie, alone);
  const start = page.getByRole('region', { name: 'Start' });
  await start.getByRole('link', { name: 'Invite people' }).waitFor();
  await page.getByRole('button', { name: 'Create channel' }).click();
  await page.getByRole('dialog', { name: 'Add someone first' }).waitFor();
  assert.equal(await page.locator('dialog.ui-dialog[open]').count(), 0, 'no form that cannot be saved');
  await page.keyboard.press('Escape');
  assert.equal(await page.evaluate(() => document.activeElement?.getAttribute('aria-label')), 'Create channel');
  app.pass('An admin alone is offered Invite people and Create agent, and Channels ＋ offers them instead of a form');

  await start.getByRole('link', { name: 'Create agent' }).click();
  await page.locator('.agent-create').waitFor();
  assert.doesNotMatch(page.url(), /new=1/);
  await page.goBack();
  await start.getByRole('button', { name: /^Ask / }).click();
  await page.locator('.chat-assistant-intro').waitFor();
  await page.getByRole('button', { name: 'Draft a message' }).click();
  assert.match(await page.locator('.message-composer textarea').inputValue(), /^Help me write a message/);
  app.pass('Create agent opens the create steps; Ask opens your assistant, whose examples fill the box');

  // A member of the seeded company: people to message, nothing to invite with.
  await page.context().close();
  page = await visit(await app.login(5), app.path.split('/').at(-1));
  assert.equal(await page.getByRole('link', { name: /Invite people/ }).count(), 0);
  assert.ok(await page.locator('.chat-start-people button').count() > 1);
  app.pass('A member sees the people already here and no Invite');
});
