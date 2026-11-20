// Chat → For you in a real browser against a real controller: what is listed,
// opening an item at its message or thread, and the counts going down as each
// is read where it is.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { until, root } from './fixtures/polling.mjs';
const { chromium } = createRequire(join(root, 'web/package.json'))('@playwright/test');

test('For you lists what is addressed to you and each item opens where it is', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 300000,
}, async t => {
  let app, browser, page;
  t.after(async () => {
    try { if (app && page && !t.passed) await page.screenshot({ path: join(app.data, 'for-you-failure.png') }).catch(() => {}); }
    finally { await browser?.close(); }
  });
  app = await workerFixture(t, {});
  const w = app.path.split('/').at(-1);
  const member = await app.login(1);
  const session = async cookie => (await app.request('/api/identity/session', cookie)).value.user;
  const [owner, colleague] = [await session(app.cookie), await session(member)];
  const channel = async name => (await app.request(`${app.path}/chats`, app.cookie, { id: randomUUID(), name, kind: 'channel', users: [colleague.id], agents: [] })).value.id;
  const say = async (cookie, chat, content, extra = {}) => { const id = randomUUID(); await app.request(`${app.path}/chats/${chat}/messages`, cookie, { id, content, ...extra }); return id; };
  const mention = text => `[@${owner.display_name ?? owner.email}](mention:user:${owner.id}) ${text}`;
  /** The owner's unread "for you" messages outside direct messages, as the server counts them. */
  const forYou = async () => (await app.request(`${app.path}/unread`, app.cookie)).value.unread
    .filter(item => item.chat !== direct).reduce((sum, item) => sum + (item.mentions ?? 0), 0);

  const orders = await channel('orders');
  await say(member, orders, 'Ordinary chatter nobody needs to answer');
  await say(member, orders, mention('please confirm the Friday delivery'));
  const design = await channel('design');
  const question = await say(app.cookie, design, 'Who has the new label artwork?');
  await say(member, design, 'I uploaded it, the blue version prints better', { thread: question });
  const direct = (await app.request(`${app.path}/chats`, member, { id: randomUUID(), name: 'Direct', kind: 'direct', users: [owner.id], agents: [] })).value.id;
  await say(member, direct, mention('also in a direct message'));
  assert.equal(await forYou(), 2);

  browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  const [name, value] = app.cookie.split('=');
  await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
  page = await context.newPage();
  await page.goto(`${app.base}/w/${w}/?view=chat`);
  const row = page.locator('a.chat-for-you');
  await row.locator('.chat-unread').getByText('2', { exact: true }).waitFor();

  // 1. The page: two new items, not the chatter, not the direct message.
  await row.click();
  const list = page.getByRole('region', { name: 'For you' });
  await list.getByRole('heading', { name: 'New · 2' }).waitFor();
  const items = list.locator('a.for-you-item');
  assert.equal(await items.count(), 2);
  const texts = await items.allTextContents();
  assert.ok(texts.some(text => text.includes('@mention') && text.includes('please confirm the Friday delivery') && text.includes('in #orders')), texts.join('\n'));
  assert.ok(texts.some(text => text.includes('Thread you follow') && text.includes('thread in #design')), texts.join('\n'));
  assert.ok(texts.every(text => !text.includes('Ordinary chatter') && !text.includes('direct message')));
  app.pass('For you lists a mention and a reply in a followed thread, and nothing else');

  // 2. A mention opens its conversation at the message, with a way back.
  await items.filter({ hasText: '@mention' }).click();
  await page.locator('.chat-pane-header').getByRole('heading', { name: 'orders' }).waitFor();
  await page.locator('a.chat-return').getByText('‹ For you').waitFor();
  await until(async () => await forYou() === 1, 'the mention was not read', 15000);
  await row.locator('.chat-unread').getByText('1', { exact: true }).waitFor();
  await page.locator('a.chat-return').click();
  await list.getByRole('heading', { name: 'New · 1' }).waitFor();
  await list.getByRole('heading', { name: 'Earlier' }).waitFor();
  app.pass('a mention opens at its message and, once read, moves to Earlier');

  // 3. A thread reply opens its thread, and reading it clears For you.
  await items.filter({ hasText: 'Thread you follow' }).click();
  await page.getByRole('dialog', { name: 'Thread', exact: true }).getByText('I uploaded it, the blue version prints better').waitFor();
  await until(async () => await forYou() === 0, 'the thread reply was not read', 15000);
  await row.locator('.chat-unread').waitFor({ state: 'detached' });
  await page.goto(`${app.base}/w/${w}/?view=chat&section=for-you`);
  await list.getByRole('heading', { name: 'Earlier' }).waitFor();
  assert.equal(await list.getByRole('heading', { name: /^New/ }).count(), 0);
  assert.equal(await items.count(), 2, 'both stay under Earlier');
  app.pass('a thread reply opens its thread; once both are read nothing is new');
});
