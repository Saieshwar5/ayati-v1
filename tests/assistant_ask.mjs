// Asking your own assistant from a channel's message box, in a real browser
// against a real controller: what you ask is never posted, its draft is yours
// alone, and "Send as me" posts it as your own message. The assistant's reply
// is the simulation's scripted one; no model is used.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { root, until } from './fixtures/polling.mjs';
const { chromium } = createRequire(join(root, 'web/package.json'))('@playwright/test');

test('a person asks their own assistant in a channel and sends its draft as themselves', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 300000,
}, async t => {
  let app, browser, page;
  t.after(async () => {
    try { if (app && page && !t.passed) await page.screenshot({ path: join(app.data, 'assistant-ask-failure.png') }).catch(() => {}); }
    finally { await browser?.close(); }
  });
  app = await workerFixture(t, {});
  const w = app.path.split('/').at(-1);
  const member = await app.login(1), colleague = await app.login(2);
  const me = (await app.request('/api/identity/session', member)).value.user;
  const them = (await app.request('/api/identity/session', colleague)).value.user;
  const sales = (await app.request(`${app.path}/chats`, app.cookie, { id: randomUUID(), name: 'sales', kind: 'channel', users: [me.id, them.id], agents: [] })).value.id;
  const question = 'How are November sales going against target?';
  await app.request(`${app.path}/chats/${sales}/messages`, colleague, { id: randomUUID(), content: question });
  const said = async cookie => (await app.request(`${app.path}/chats/${sales}/messages`, cookie)).value.messages;

  browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  const [name, value] = member.split('=');
  await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
  page = await context.newPage();
  await page.goto(`${app.base}/w/${w}/?view=chat`);
  await page.locator('a.chat-contact', { hasText: 'sales' }).click();
  await page.locator('.chat-pane-header').getByRole('heading', { name: 'sales' }).waitFor();

  // Whatever the assistant is called, it is the first row of the @ list and is found as "my".
  const mine = (await app.request(`${app.path}/chat-directory`, member)).value.agents.find(agent => agent.assistant && agent.yours);
  const box = page.locator('.message-composer textarea');
  await box.pressSequentially('@');
  const first = page.getByRole('option').first();
  await first.waitFor();
  assert.match(await first.textContent(), new RegExp(`${mine.name}.*Your assistant · Only you see what you ask`));
  await box.pressSequentially('my');
  assert.equal(await page.getByRole('option').count(), 1);
  await box.press('Enter');
  assert.equal(await box.inputValue(), `@${mine.name} `);
  await box.pressSequentially('tell them we are on track');
  await page.getByText(`Asking ${mine.name}`).waitFor();
  await page.getByText('Nothing is sent in #sales.').waitFor();
  const ask = page.getByRole('button', { name: `Ask ${mine.name}`, exact: true }).and(page.locator('[type=submit]'));
  await ask.waitFor();
  app.pass('Mentioning your assistant turns the message box into a private request');

  await box.press('Enter');
  const card = page.locator('.assistant-ask');
  await card.waitFor();
  assert.equal(await box.inputValue(), '', 'the box is free while it writes');
  assert.deepEqual((await said(colleague)).map(message => message.content), [question], 'what was asked is not in the channel');
  await card.getByText(`Draft from ${mine.name}`).waitFor({ timeout: 60000 });
  await card.getByText('Will be sent as you in #sales').waitFor();
  const draft = (await card.locator('.assistant-ask-text').textContent()).trim();
  assert.ok(draft.length > 0);
  assert.equal((await said(colleague)).length, 1, 'a written draft is still only yours');
  app.pass('The draft arrives where only its person sees it; the channel is unchanged');

  await card.getByRole('button', { name: 'Send as me' }).click();
  await card.waitFor({ state: 'detached' });
  const sent = await until(async () => (await said(colleague)).find(message => message.author === me.id), 'The draft was not sent');
  assert.equal(sent.kind, 'user');
  assert.equal(sent.content.trim(), draft);
  assert.ok(!sent.assisted, 'nobody else learns an assistant wrote it');
  await until(async () => (await said(member)).find(message => message.id === sent.id)?.assisted?.by === 'assistant', 'The private record was not kept');
  await page.locator('.chat-message-own', { hasText: draft.slice(0, 40) }).waitFor();
  app.pass('Send as me posts the draft as the person’s own message, marked for them alone');

  // A name typed by hand is not a mention: the box warns before those words are posted.
  await box.fill(`@${mine.name.toLowerCase()} keep this between us`);
  await page.getByText('This would be sent in #sales as written.').waitFor();
  await page.getByRole('button', { name: `Ask ${mine.name} instead` }).click();
  await page.getByText(`Asking ${mine.name}`).waitFor();
  await box.fill('');
  // And the server refuses a posted message that mentions your assistant.
  const refused = await app.request(`${app.path}/chats/${sales}/messages`, member,
    { id: randomUUID(), content: `[@${mine.name}](mention:agent:${mine.id}) keep this between us` }, 400);
  assert.match(JSON.stringify(refused.value), /asks your assistant/);
  assert.equal((await said(colleague)).length, 2);
  app.pass('A typed name warns first, and a posted assistant mention is refused');
});
