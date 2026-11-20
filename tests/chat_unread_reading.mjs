// Reading clears the unread count, in a real browser against a real controller:
// plain messages, a message taller than the screen, and replies inside a thread,
// which stay unread until the thread is opened and are marked new on its line.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { until, root } from './fixtures/polling.mjs';
const { chromium } = createRequire(join(root, 'web/package.json'))('@playwright/test');

test('reading a conversation clears its unread count, and thread replies say they are new', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 300000,
}, async t => {
  let app, browser, page;
  t.after(async () => {
    try { if (app && page && !t.passed) await page.screenshot({ path: join(app.data, 'unread-reading-failure.png') }).catch(() => {}); }
    finally { await browser?.close(); }
  });
  app = await workerFixture(t, {});
  const w = app.path.split('/').at(-1);
  const member = await app.login(1);
  const memberUser = (await app.request('/api/identity/session', member)).value.user;
  const channel = async name => (await app.request(`${app.path}/chats`, app.cookie, { id: randomUUID(), name, kind: 'channel', users: [memberUser.id], agents: [] })).value.id;
  const say = async (chat, content, extra = {}) => { const id = randomUUID(); await app.request(`${app.path}/chats/${chat}/messages`, member, { id, content, ...extra }); return id; };
  /** The owner's unread count in one conversation, as the server counts it. */
  const unread = async chat => (await app.request(`${app.path}/unread`, app.cookie)).value.unread.find(item => item.chat === chat)?.count ?? 0;
  const cleared = (chat, count = 0) => until(async () => await unread(chat) === count, `unread did not reach ${count}`, 15000);

  const plain = await channel('plain');
  for (const text of ['One', 'Two', 'Three']) await say(plain, text);
  const tall = await channel('tall');
  await say(tall, Array.from({ length: 160 }, (_, line) => `Line ${line + 1} of a long pasted note`).join('\n\n'));
  const threaded = await channel('threaded');
  const question = await say(threaded, 'Can someone check the Friday delivery?');
  for (const text of ['First reply inside the thread', 'Second reply inside the thread']) await say(threaded, text, { thread: question });
  assert.deepEqual([await unread(plain), await unread(tall), await unread(threaded)], [3, 1, 3]);

  browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  const [name, value] = app.cookie.split('=');
  await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
  page = await context.newPage();
  const open = async chat => { await page.goto(`${app.base}/w/${w}/?view=chat&conversation=${chat}`); await page.locator('.chat-message-list article').first().waitFor(); };
  const row = label => page.locator('a.chat-contact').filter({ hasText: label });

  // 1. Short messages on screen are read.
  await open(plain);
  await cleared(plain);
  await row('plain').locator('.chat-unread').waitFor({ state: 'detached' });
  app.pass('reading short messages clears the count');

  // 2. A message much taller than the list is read from one screenful of it.
  await open(tall);
  const sizes = await page.locator('.chat-message-list').evaluate(list => ({ area: list.clientHeight, message: list.querySelector('article').getBoundingClientRect().height }));
  assert.ok(sizes.message > sizes.area / 0.6, `the message (${sizes.message}px) cannot be 60% visible in ${sizes.area}px`);
  await cleared(tall);
  await row('tall').locator('.chat-unread').waitFor({ state: 'detached' });
  app.pass('a message taller than the screen is read');

  // 3. Reading the channel reads the question, not the replies in its thread; the line points at them.
  await open(threaded);
  await cleared(threaded, 2);
  const line = page.locator('.chat-message-list .chat-thread-line');
  await line.locator('.chat-thread-new').getByText('2 new', { exact: true }).waitFor();
  assert.match(await line.getAttribute('aria-label'), /^Open thread: 2 replies · 2 new · /);
  // The list reads the counts again shortly after the receipts' changes arrive.
  await row('threaded').locator('.chat-unread').getByText('2', { exact: true }).waitFor();
  await line.click();
  await cleared(threaded);
  await line.locator('.chat-thread-new').waitFor({ state: 'detached' });
  await row('threaded').locator('.chat-unread').waitFor({ state: 'detached' });
  await page.locator('.workspace-rail .workspace-nav-unread').waitFor({ state: 'detached' });
  app.pass('thread replies are marked new and are read in the thread');

  // 4. A reply that arrives while the channel is open (thread closed) is marked new at once.
  await page.getByRole('dialog', { name: 'Thread', exact: true }).getByRole('button', { name: 'Close', exact: true }).click();
  await say(threaded, 'A third reply, while you are reading the channel', { thread: question });
  await line.locator('.chat-thread-new').getByText('1 new', { exact: true }).waitFor();
  await cleared(threaded, 1);
  app.pass('a reply arriving in a closed thread is marked new');
});
