// Leaving Chat for another page and coming back reopens the conversation you
// had open, in a real browser against a real controller, instead of the
// first-visit start (your assistant, Invite people, Create agent).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { root } from './fixtures/polling.mjs';
const { chromium } = createRequire(join(root, 'web/package.json'))('@playwright/test');

test('coming back to Chat reopens the last conversation', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 300000,
}, async t => {
  let app, browser, page;
  t.after(async () => {
    try { if (app && page && !t.passed) await page.screenshot({ path: join(app.data, 'chat-return-failure.png') }).catch(() => {}); }
    finally { await browser?.close(); }
  });
  app = await workerFixture(t, {});
  const w = app.path.split('/').at(-1);
  const member = await app.login(1);
  const colleague = (await app.request('/api/identity/session', member)).value.user;
  await app.request(`${app.path}/chats`, app.cookie, { id: randomUUID(), name: 'orders', kind: 'channel', users: [colleague.id], agents: [] });

  browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  const [name, value] = app.cookie.split('=');
  await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
  page = await context.newPage();
  await page.goto(`${app.base}/w/${w}/?view=chat`);
  const welcome = page.getByRole('region', { name: 'Start' });
  const header = page.locator('.chat-pane-header').getByRole('heading', { name: 'orders' });
  const rail = label => page.locator(`.workspace-rail [title=${label}]`);
  await welcome.waitFor();

  await page.locator('a.chat-contact', { hasText: 'orders' }).click();
  await header.waitFor();
  await rail('Home').click();
  await page.locator('.home').waitFor();
  await rail('Chat').click();
  await header.waitFor();
  assert.equal(await welcome.count(), 0);
  assert.match(page.url(), /\?view=chat&conversation=/);
  app.pass('Home → Chat reopens the conversation that was open');

  await page.goBack();
  await page.locator('.home').waitFor();
  await page.reload();
  await page.locator('.home').waitFor();
  await rail('Chat').click();
  await header.waitFor();
  app.pass('Back returns to Home in one step, and the tab still remembers after a reload');
});
