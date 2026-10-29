import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { writeFile } from 'node:fs/promises';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { root, until } from './fixtures/polling.mjs';

const { chromium } = createRequire(join(root, 'web/package.json'))('playwright');
test('context preview shows only current permitted evidence, stays read-only and clears an open revoked view', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 150000,
}, async t => {
  let app, browser;
  const pages = [], errors = [];
  t.after(async () => {
    if (app && !t.passed) for (const [i, page] of pages.entries()) {
      await page.screenshot({ path: join(app.data, `context-preview-failure-${i}.png`) }).catch(() => {});
      await writeFile(join(app.data, `context-preview-failure-${i}.html`), await page.content()).catch(() => {});
    }
    await browser?.close();
  });
  app = await workerFixture(t, { mode: 'pooled', control: true, leaseSeconds: 15 });
  await app.request(`${app.path}/agent-profiles`, app.cookie, {
    ...app.profile, revision: 1, settings: { ...app.profile.settings, memory: true },
  });
  const member = await app.login(1);
  const memberId = (await app.request('/api/identity/session', member)).value.user.id;
  await app.request(`${app.path}/chat-grants`, app.cookie, { user: memberId, agent: app.profile.id, allowed: true });
  const memberChat = (await app.request(`${app.path}/chats`, member, {
    id: randomUUID(), kind: 'direct', name: 'My member conversation', users: [], agents: [app.profile.id],
  })).value.id;
  const ownerMessage = '<img src=x onerror=alert(1)> OWNER_PRIVATE_CONTEXT ' + 'Earlier approved discussion. '.repeat(35);
  for (const [chat, cookie, content] of [[app.conversation, app.cookie, ownerMessage], [memberChat, member, 'MEMBER_PRIVATE_CONTEXT']]) {
    await app.request(`${app.path}/chats/${chat}/messages`, cookie, { id: randomUUID(), content });
  }
  const knowledge = `${app.path}/agent-profiles/${app.profile.id}/knowledge`;
  const note = { id: randomUUID(), title: 'Private working preference', content: 'OWNER_PRIVATE_APPROVED_NOTE',
    chat: app.conversation, source_message: null, revision: 0, forgotten: false };
  await app.request(knowledge, app.cookie, note);
  const endpoint = `${app.path}/agent-profiles/${app.profile.id}/context-preview`;
  await app.request(`${endpoint}?chat=${app.conversation}`, member, undefined, 400);
  await app.request(`${endpoint}?chat=${memberChat}`, app.cookie, undefined, 400);
  await app.request(`${endpoint}?chat=${app.conversation}&requester=${memberId}`, app.cookie, undefined, 400);
  const preview = (await app.request(`${endpoint}?chat=${app.conversation}`, app.cookie)).value;
  assert.equal(preview.notes[0].id, note.id);
  assert.equal(preview.messages[0].content, ownerMessage);
  assert.ok(!JSON.stringify(preview).includes('MEMBER_PRIVATE_CONTEXT'));
  const jobs = () => Number(app.sql('SELECT count(*) FROM relay_chat_jobs'));
  assert.equal(jobs(), 0);
  browser = await chromium.launch({ headless: true });
  async function open(cookie) {
    const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
    const [name, value] = cookie.split('=');
    await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
    const page = await context.newPage(); pages.push(page);
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(`${app.base}/w/${app.path.split('/').at(-1)}/?view=agents&selected=${app.profile.id}`);
    await page.getByRole('tab', { name: 'Knowledge', exact: true }).click();
    await page.getByRole('button', { name: 'Preview conversation context', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'Conversation context preview', exact: true });
    await dialog.waitFor();
    assert.equal(await dialog.getByRole('combobox').inputValue(), '');
    return { page, dialog };
  }
  const { page, dialog } = await open(app.cookie);
  assert.equal(await dialog.getByRole('combobox').locator(`option[value="${memberChat}"]`).count(), 0);
  await dialog.getByRole('combobox').selectOption(app.conversation);
  await dialog.getByRole('heading', { name: 'Recent messages · 1', exact: true }).waitFor();
  assert.equal(await dialog.locator('img').count(), 0);
  for (const time of await dialog.locator('time').all()) assert.ok((await time.getAttribute('datetime')).startsWith(String(new Date().getUTCFullYear())));
  await dialog.getByText('Read full text', { exact: true }).click();
  assert.ok((await dialog.innerText()).includes(ownerMessage));
  await dialog.getByText('OWNER_PRIVATE_APPROVED_NOTE', { exact: true }).waitFor();
  await dialog.getByRole('button', { name: 'Show less', exact: true }).click();
  await page.screenshot({ path: join(app.data, 'context-preview-desktop.png') });
  await dialog.getByRole('button', { name: 'Refresh preview', exact: true }).click();
  await dialog.getByRole('heading', { name: 'Recent messages · 1', exact: true }).waitFor();
  assert.equal(jobs(), 0); assert.equal(app.provider.requests.length, 0);
  for (const [width, height, theme] of [[720, 640, 'light'], [390, 844, 'dark']]) {
    await page.setViewportSize({ width, height });
    await page.evaluate(theme => { document.documentElement.dataset.theme = theme; }, theme);
    await dialog.getByRole('heading', { name: 'Approved notes · 1', exact: true }).scrollIntoViewIfNeeded();
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
    await page.screenshot({ path: join(app.data, `context-preview-${width}-${theme}.png`) });
  }
  await page.keyboard.press('Escape');
  await dialog.waitFor({ state: 'detached' });
  assert.equal(await page.getByRole('button', { name: 'Preview conversation context', exact: true })
    .evaluate(node => node === document.activeElement), true);
  app.pass('Preview uses current scoped backend evidence without inference or jobs; untrusted text remains inert, long text opens deliberately and desktop/narrow/dark drawers restore focus');

  const viewer = await open(member);
  await viewer.dialog.getByRole('combobox').selectOption(memberChat);
  await viewer.dialog.getByText('MEMBER_PRIVATE_CONTEXT', { exact: true }).waitFor();
  assert.ok(!(await viewer.dialog.innerText()).includes('OWNER_PRIVATE'));
  await app.request(`${app.path}/chat-grants`, app.cookie, { user: memberId, agent: app.profile.id, allowed: false });
  await until(async () => !(await viewer.page.content()).includes('MEMBER_PRIVATE_CONTEXT'), 'Revoked preview retained private text', 15000);
  await viewer.dialog.waitFor({ state: 'detached' });
  await app.request(`${endpoint}?chat=${memberChat}`, member, undefined, 400);
  assert.equal(jobs(), 0); assert.equal(app.provider.requests.length, 0);
  await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  await app.done(await app.send('Use my currently approved working preference.'));
  const actual = JSON.stringify(app.provider.requests[0].messages);
  assert.ok(actual.includes('OWNER_PRIVATE_APPROVED_NOTE'));
  assert.ok(!actual.includes('MEMBER_PRIVATE_CONTEXT'));
  assert.equal(app.provider.requests.length, 1);
  const oldKnowledgeCount = preview.notes.length;
  await app.request(knowledge, app.cookie, { ...note, forgotten: true });
  const updated = (await app.request(`${endpoint}?chat=${app.conversation}`, app.cookie)).value;
  assert.equal(updated.notes.length, oldKnowledgeCount - 1);
  assert.deepEqual(errors, []);
  await writeFile(join(app.data, 'context-preview.json'), JSON.stringify({ passed: true,
    no_preview_jobs: true, no_preview_inference: true, current_scope: true, inert_content: true,
    revoked_open_view_cleared: true, actual_execution_note_matches: true, forgotten_note_removed: true,
    sizes: [1280, 720, 390] }, null, 2));
  app.pass('Another person previews only their own context; revocation clears an open view, real Goose receives the same allowed note selection and forgetting updates the next preview');
});
