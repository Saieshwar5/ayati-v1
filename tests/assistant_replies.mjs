// Your assistant in a direct chat, in a real browser against a real controller
// (schema 141): the header switch, asking for a reply by mentioning it, the
// @name tag on what it wrote, and a reply from one message. The controller
// runs scripted replies and no executor, so nothing is handed over on its own
// here: that part is covered by the PostgreSQL tests of assistant_reply_modes.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { root } from './fixtures/polling.mjs';
const { chromium } = createRequire(join(root, 'web/package.json'))('@playwright/test');

test('the reply switch, a reply asked by mention and its tag work in a real browser', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 300000,
}, async t => {
  let app, browser, page;
  const shot = name => page.screenshot({ path: join(app.data, `assistant-replies-${name}.png`) }).catch(() => {});
  t.after(async () => {
    try { if (app && page && !t.passed) await shot('failure'); }
    finally { await browser?.close(); }
  });
  app = await workerFixture(t, {});
  const w = app.path.split('/').at(-1);
  const member = await app.login(1);
  const colleague = (await app.request('/api/identity/session', member)).value.user;
  // Reading the chat list makes the signed-in member's assistant.
  await app.request(`${app.path}/chats`, app.cookie);
  const direct = (await app.request(`${app.path}/chats`, app.cookie, { id: randomUUID(), name: 'Direct message', kind: 'direct', users: [colleague.id], agents: [] })).value.id;
  const asked = randomUUID();
  await app.request(`${app.path}/chats/${direct}/messages`, member, { id: asked, content: 'Can we meet on Friday at 3?' });
  const threads = async () => (await app.request(`${app.path}/assistant/threads`, app.cookie)).value.threads;
  const messages = async who => (await app.request(`${app.path}/chats/${direct}/messages`, who)).value.messages;

  browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  const [name, value] = app.cookie.split('=');
  await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
  page = await context.newPage();
  await page.goto(`${app.base}/w/${w}/?view=chat&conversation=${direct}`);
  const toggle = page.locator('.chat-replies > button');
  await toggle.waitFor();
  const label = await toggle.getAttribute('aria-label');
  const assistant = label.replace(' in this chat: Off', '');
  assert.match(label, / in this chat: Off$/);
  const choose = async choice => { await toggle.click(); await page.getByRole('radio', { name: new RegExp(`^${choice}`) }).click(); };

  await choose('Ask me first');
  await page.locator('.chat-thread-pill.ask', { hasText: `${assistant} asks you first` }).waitFor();
  await page.locator('a.chat-contact .chat-thread-mark.ask').waitFor();
  assert.deepEqual((await threads()).map(thread => [thread.state, thread.mode, thread.chat]), [['running', 'ask', direct]]);
  await shot('ask-me-first');
  app.pass('Ask me first is one click: the header and the chat list say so');

  await choose('Reply for me');
  assert.equal((await threads())[0].mode, 'ask', 'choosing it starts nothing');
  await page.locator('.chat-replies-start textarea').fill('Settle a time for Friday.');
  await shot('reply-for-me-start');
  await page.getByRole('button', { name: 'Start replying' }).click();
  await page.locator('.chat-thread-pill.auto', { hasText: `${assistant} is replying` }).waitFor();
  await page.locator('.chat-took-over', { hasText: `${assistant} replies for you from here` }).waitFor();
  const [replying] = await threads();
  assert.deepEqual([replying.mode, replying.instruction, replying.anchor], ['auto', 'Settle a time for Friday.', asked]);
  await shot('replying');
  app.pass('Reply for me starts from its own button, from the unanswered message');

  await choose('Off');
  await page.locator('.chat-replies-button').waitFor();
  assert.equal((await threads())[0].state, 'stopped');
  assert.equal(await page.locator('a.chat-contact .chat-thread-mark').count(), 0);
  app.pass('Off removes the pill and the mark');

  // A message that mentions the assistant asks it and is never posted.
  const box = page.locator('.message-composer textarea');
  await box.click();
  await box.pressSequentially(`@${assistant.slice(0, 3)}`);
  await page.getByRole('option', { name: /only you see this/ }).click();
  await box.pressSequentially('say Friday at 3 works');
  const send = page.locator('.message-composer button[type=submit]');
  assert.equal(await send.textContent(), `Ask ${assistant}`);
  await page.locator('.composer-notice', { hasText: 'Nothing is sent.' }).waitFor();
  await shot('asking');
  await box.press('Enter');
  await page.locator('.composer-notice', { hasText: /is writing a draft|Draft from/ }).waitFor();
  assert.equal((await messages(app.cookie)).length, 1, 'nothing was posted in the chat');
  app.pass('Mentioning the assistant asks it and posts nothing');

  await page.locator('.composer-notice', { hasText: `Draft from ${assistant}` }).waitFor({ timeout: 90000 });
  const drafted = await box.inputValue();
  assert.ok(drafted.trim() && !drafted.includes(`@${assistant}`), 'the reply is in the message box');
  await shot('draft-ready');
  await box.press('Enter');
  const tag = page.locator('.chat-message-own .chat-assisted');
  await tag.waitFor({ timeout: 30000 });
  assert.equal(await tag.textContent(), `Written with @${assistant}`);
  const sent = (await messages(member)).at(-1);
  assert.equal(sent.author, (await app.request('/api/identity/session', app.cookie)).value.user.id);
  assert.equal(sent.assisted ?? null, null, 'the other side gets an ordinary message');
  await shot('tag');
  app.pass('The reply you send is yours, tagged for you alone');

  // One reply to a chosen message.
  await page.locator(`[data-message-id="${asked}"]`).hover();
  await page.getByRole('button', { name: `Reply with @${assistant}` }).first().click();
  assert.equal(await box.inputValue(), `@${assistant} `);
  await page.locator('.chat-replying', { hasText: 'Can we meet on Friday at 3?' }).waitFor();
  await shot('reply-with');
  app.pass('Reply with @name quotes the message and mentions the assistant');
});
