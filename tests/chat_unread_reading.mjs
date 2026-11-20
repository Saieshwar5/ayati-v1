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
  await page.locator('.workspace-rail [title=Chat] .workspace-nav-unread').waitFor({ state: 'detached' });
  app.pass('thread replies are marked new and are read in the thread');

  // 4. A reply that arrives while the channel is open (thread closed) is marked new at once.
  await page.getByRole('dialog', { name: 'Thread', exact: true }).getByRole('button', { name: 'Close', exact: true }).click();
  await say(threaded, 'A third reply, while you are reading the channel', { thread: question });
  await line.locator('.chat-thread-new').getByText('1 new', { exact: true }).waitFor();
  await cleared(threaded, 1);
  app.pass('a reply arriving in a closed thread is marked new');

  // 5. A thread far older than the loaded history: the bar above the messages opens it.
  const archive = await channel('archive');
  const old = await say(archive, 'An old question about March stock');
  await say(archive, 'A reply nobody has read yet', { thread: old });
  for (let n = 0; n < 205; n++) await app.request(`${app.path}/chats/${archive}/messages`, app.cookie, { id: randomUUID(), content: `Newer note ${n}` });
  await cleared(archive, 2);
  await open(archive);
  const bar = page.getByRole('region', { name: 'Threads with new replies' });
  await bar.getByText('New replies in 1 thread', { exact: true }).waitFor();
  await bar.getByRole('button', { name: /^Open thread “An old question about March stock”: 1 new reply$/ }).click();
  await page.getByRole('dialog', { name: 'Thread', exact: true }).getByText('A reply nobody has read yet').waitFor();
  await cleared(archive);
  await bar.waitFor({ state: 'detached' });
  app.pass('an unread reply under a question older than the loaded history is listed and read from the bar');

  // 6. Muted on another device: this page's bell, the rail and the tab title follow.
  const other = await channel('other device');
  await say(other, 'Something on a channel you are about to mute');
  await page.goto(`${app.base}/w/${w}/?view=home`);
  const chatCount = page.locator('.workspace-rail [title=Chat] .workspace-nav-unread');
  await chatCount.getByText('2', { exact: true }).waitFor();
  await until(async () => (await page.title()) === '(2) Home · Cuckoo', 'the tab title shows what is waiting', 15000);
  await app.request(`${app.path}/chats/${other}/notifications`, app.cookie, { level: 'none' });
  await chatCount.getByText('1', { exact: true }).waitFor({ timeout: 15000 });
  await until(async () => (await page.title()) === '(1) Home · Cuckoo', 'the tab title follows the mute', 15000);
  await page.locator('.workspace-rail [title=Chat]').click();
  await row('other device').locator('.chat-muted').waitFor();
  app.pass('a mute made on another device reaches the open page');

  // 7. A visible window that is not focused reads only while it is used.
  await open(plain);
  // Headless Chromium reports every page focused, so this stands in for the
  // operating system moving focus to another app; the page stays visible.
  await page.evaluate(() => { document.hasFocus = () => false; window.dispatchEvent(new Event('blur')); });
  assert.equal(await page.evaluate(() => document.hidden), false, 'the window is still visible');
  await say(plain, 'Sent while you were typing in another window');
  await page.getByText('Sent while you were typing in another window').waitFor();
  await page.waitForTimeout(3000);
  assert.equal(await unread(plain), 1, 'not read while nobody uses that window');
  await page.mouse.move(640, 400); await page.mouse.move(660, 420);
  await cleared(plain);
  app.pass('a visible, unfocused window reads once the person uses it');
});
