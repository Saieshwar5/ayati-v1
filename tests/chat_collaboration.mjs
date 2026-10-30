// The whole people-and-agents story in real browsers against a real
// controller and executor: profiles, the directory, direct messages, pins,
// desktop alerts, channels with an agent, reactions, edits and the access
// boundary for someone outside a private channel. Members join through the
// simulation's invite and accept path; email delivery needs hosted WorkOS.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { writeFile } from 'node:fs/promises';
import { deflateSync } from 'node:zlib';
import { join } from 'node:path';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { until, root } from './fixtures/polling.mjs';
const { chromium } = createRequire(join(root, 'web/package.json'))('@playwright/test');

/** A small, valid PNG for the profile photo. */
function png() {
  const crc = buffer => { let c = ~0; for (const byte of buffer) { c ^= byte; for (let k = 0; k < 8; k++) c = (c >>> 1) ^ (0xedb88320 & -(c & 1)); } return ~c >>> 0; };
  const chunk = (type, data) => { const head = Buffer.alloc(8); head.writeUInt32BE(data.length); head.write(type, 4); const tail = Buffer.alloc(4); tail.writeUInt32BE(crc(Buffer.concat([Buffer.from(type), data]))); return Buffer.concat([head, data, tail]); };
  const header = Buffer.alloc(13); header.writeUInt32BE(32, 0); header.writeUInt32BE(32, 4); header[8] = 8; header[9] = 2;
  const rows = Buffer.concat(Array.from({ length: 32 }, () => Buffer.concat([Buffer.from([0]), Buffer.alloc(96, 120)])));
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', header), chunk('IDAT', deflateSync(rows)), chunk('IEND', Buffer.alloc(0))]);
}

test('people and agents work together in chat, and private channels stay private', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 300000,
}, async t => {
  let app, browser; const pages = [], errors = [];
  t.after(async () => {
    try { if (app && !t.passed) for (const [index, page] of pages.entries()) await page.screenshot({ path: join(app.data, `collaboration-failure-${index}.png`) }).catch(() => {}); }
    finally { await browser?.close(); }
    if (app) await writeFile(join(app.data, 'collaboration-errors.json'), JSON.stringify(errors, null, 2));
  });
  app = await workerFixture(t, { mode: 'pooled', control: true, capacity: 1, workspaceLimit: 1, modelLimit: 1, leaseSeconds: 15 });
  await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  const w = app.path.split('/').at(-1);
  const [member, outsider] = [await app.login(1), await app.login(2)];
  const me = async cookie => (await app.request('/api/identity/session', cookie)).value.user;
  const [ownerUser, memberUser] = [await me(app.cookie), await me(member)];
  browser = await chromium.launch({ headless: true });
  async function openPage(cookie, alerts = false) {
    const context = await browser.newContext({ viewport: { width: 1440, height: 960 } });
    // Record desktop alerts instead of showing them. The headless browser cannot
    // grant notifications, so this stands in for a person who allowed them.
    await context.addInitScript(allowed => {
      window.__alerts = [];
      window.Notification = class { constructor(title, options) { window.__alerts.push({ title, body: options?.body }); } close() {} static get permission() { return allowed ? 'granted' : 'default'; } static requestPermission() { return Promise.resolve(allowed ? 'granted' : 'denied'); } };
    }, alerts);
    const [name, value] = cookie.split('=');
    await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
    const page = await context.newPage(); page.on('pageerror', error => errors.push(error.message)); pages.push(page); return page;
  }
  const at = view => `${app.base}/w/${w}/?view=${view}`;
  const owner = await openPage(app.cookie), colleague = await openPage(member, true);

  // 1. The member describes themselves: title, introduction and photo.
  await colleague.goto(at('settings&scope=personal&section=my-account'));
  await colleague.getByLabel('Job title').fill('Warehouse lead');
  await colleague.getByLabel('About').fill('Ask me about stock and deliveries.');
  await colleague.getByRole('button', { name: 'Save profile' }).click();
  await colleague.getByText('Profile saved.').waitFor();
  await colleague.getByLabel('Choose a profile photo').setInputFiles({ name: 'me.png', mimeType: 'image/png', buffer: png() });
  await colleague.getByText('Photo updated.').waitFor();
  app.pass('a member saves a profile with a photo');

  // 2. The owner finds them in the directory, sees the profile and pins them.
  await colleague.goto(at('home'));
  await owner.goto(at('chat'));
  await owner.getByRole('button', { name: /People & agents/ }).click();
  const directory = owner.getByRole('dialog', { name: 'People & agents' });
  const name = memberUser.display_name ?? memberUser.email;
  const row = directory.locator('li').filter({ has: owner.getByRole('button', { name: `View profile of ${name}`, exact: true }) });
  await row.getByText('Warehouse lead').waitFor();
  assert.equal(await row.locator('img').count(), 1, 'the photo is shown');
  assert.ok(await directory.locator('li').count() >= 3, 'everyone in the workspace is listed, with the agent');
  await row.getByRole('button', { name: `Pin ${name}` }).click();
  await row.getByRole('button', { name: `Unpin ${name}` }).waitFor();
  await row.getByRole('button', { name: 'Message' }).click();
  await owner.locator('[aria-label="Pinned conversations"]').getByText(name).waitFor();
  app.pass('the directory lists everyone; pinning a person pins the conversation');

  // 3. A direct message alerts the member on another page.
  const hello = `Hello from the owner ${randomUUID().slice(0, 6)}`;
  await owner.getByRole('textbox', { name: 'Message', exact: true }).fill(hello);
  await owner.getByRole('button', { name: 'Send message', exact: true }).click();
  await colleague.waitForFunction(text => window.__alerts.some(alert => alert.body === text), hello, { timeout: 20000 });
  app.pass('a direct message raises a desktop alert');

  // 4. The owner creates a private channel with the member and adds the agent.
  await owner.goto(at('chat'));
  await owner.getByRole('button', { name: 'Create channel' }).click();
  const create = owner.getByRole('dialog', { name: 'Create channel' });
  await create.getByLabel('Channel name').fill('Orders');
  await create.getByRole('checkbox', { name: new RegExp(name) }).check();
  await create.getByRole('button', { name: 'Create channel' }).click();
  await owner.getByRole('heading', { name: 'Orders', exact: true }).waitFor();
  const channel = new URL(owner.url()).searchParams.get('conversation');
  await owner.getByRole('button', { name: 'Details', exact: true }).click();
  const details = owner.getByRole('dialog', { name: 'Conversation details' });
  await details.getByRole('checkbox', { name: /Nova/ }).check();
  await details.getByRole('button', { name: 'Save agents' }).click();
  await details.getByText('Agents saved.').waitFor();
  await details.getByRole('button', { name: 'Close' }).click();
  app.pass('a channel manager adds an agent to an existing channel');

  // 5. The member asks the agent; it works and answers in the channel. Asking a
  // shared agent still needs the owner's grant, separate from channel membership.
  await app.request(`${app.path}/chat-grants`, app.cookie, { user: memberUser.id, agent: app.profile.id, allowed: true });
  await colleague.goto(`${at('chat')}&conversation=${channel}`);
  const box = colleague.getByRole('textbox', { name: 'Message', exact: true });
  await until(async () => (await app.request(`${app.path}/chats`, member)).value.chats.find(chat => chat.id === channel)?.agents.length === 1, 'Agent did not join', 10000);
  await colleague.reload(); await box.waitFor();
  await box.fill('@Nov'); await colleague.getByRole('option', { name: /Nova/ }).click();
  await box.pressSequentially('please check the stock');
  await colleague.getByRole('button', { name: 'Send message', exact: true }).click();
  const answer = colleague.locator('article', { hasText: 'GOOSE_FIXTURE_OK' });
  await answer.waitFor({ timeout: 60000 });
  assert.ok(await answer.locator('.ai-tag').count() >= 1, 'the answer is marked as from an AI agent');
  app.pass('a member asks the agent in a channel and it answers');

  // 6. The owner reacts; the member sees it live. The member edits a message.
  await owner.goto(`${at('chat')}&conversation=${channel}`);
  const ownersView = owner.locator('article', { hasText: 'GOOSE_FIXTURE_OK' });
  // The hover toolbar stays inside the message, so the list never scrolls sideways.
  const sideways = page => page.locator('.chat-message-list').evaluate(list => list.scrollWidth > list.clientWidth);
  await ownersView.hover(); assert.equal(await sideways(owner), false, 'a hovered agent message widens the list');
  await owner.screenshot({ path: join(app.data, 'collaboration-hover-desktop.png') });
  await ownersView.getByRole('button', { name: 'React 👍' }).click();
  await answer.getByRole('button', { name: /^👍 1/ }).waitFor();
  const request = colleague.locator('article.chat-message-own', { hasText: 'please check the stock' });
  await request.hover(); await request.getByRole('button', { name: 'More actions' }).click();
  assert.equal(await sideways(colleague), false, 'an open menu on an own message widens the list');
  await request.getByRole('menuitem', { name: 'Edit message' }).click();
  await request.getByRole('textbox', { name: 'Edit message' }).fill('please check the stock today');
  await request.getByRole('button', { name: 'Save' }).click();
  await owner.locator('article', { hasText: 'please check the stock today' }).getByText('edited', { exact: true }).waitFor();
  app.pass('reactions and edits reach the other person live');

  // 7. Someone outside the private channel sees everyone, but not the channel.
  const others = await openPage(outsider);
  const chats = (await app.request(`${app.path}/chats`, outsider)).value.chats;
  assert.equal(chats.some(chat => chat.id === channel), false);
  const denied = await app.request(`${app.path}/chats/${channel}/messages`, outsider, undefined, 400);
  assert.equal(denied.value.error, 'Conversation access denied');
  const everyone = (await app.request(`${app.path}/chat-directory`, outsider)).value.users.map(user => user.id);
  assert.ok(everyone.includes(ownerUser.id) && everyone.includes(memberUser.id), 'everyone is visible to everyone');
  await others.goto(`${at('chat')}&conversation=${channel}`);
  await others.getByText('This conversation is unavailable or you no longer have access.').waitFor();
  app.pass('a private channel stays private while people stay visible');

  // 8. The same screens fit a phone.
  await colleague.setViewportSize({ width: 390, height: 844 });
  await colleague.goto(`${at('chat')}&conversation=${channel}`); await box.waitFor();
  await colleague.locator('article', { hasText: 'GOOSE_FIXTURE_OK' }).waitFor();
  const header = await colleague.locator('.chat-pane-header h2').boundingBox();
  assert.ok(header.height < 30, 'the conversation title stays on one line');
  assert.equal(await colleague.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
  await colleague.locator('article', { hasText: 'GOOSE_FIXTURE_OK' }).hover();
  assert.equal(await sideways(colleague), false, 'a hovered message widens the list on a phone');
  await colleague.screenshot({ path: join(app.data, 'collaboration-hover-phone.png') });
  await colleague.screenshot({ path: join(app.data, 'collaboration-phone.png') });
  assert.deepEqual(errors, []);
  app.pass('phone layout without page overflow and no page errors');
});
