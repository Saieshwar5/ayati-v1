import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { writeFile } from 'node:fs/promises';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { root, until } from './fixtures/goose_app.mjs';

const { chromium } = createRequire(join(root, 'web/package.json'))('playwright');
const promptText = request => request.messages.flatMap(message => typeof message.content === 'string'
  ? [message.content] : (message.content ?? []).filter(c => c.type === 'text').map(c => c.text)).join('\n');
const writeFields = note => Object.fromEntries(['id', 'title', 'content', 'chat', 'source_message', 'revision', 'forgotten'].map(key => [key, note[key]]));

test('approved knowledge requires a chosen audience, edits and forgets real records, and stays scoped in actual Goose input', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 180000,
}, async t => {
  let app, browser, page;
  t.after(async () => {
    if (app && page && !t.passed) {
      await page.screenshot({ path: join(app.data, 'knowledge-failure.png') }).catch(() => {});
      await writeFile(join(app.data, 'knowledge-failure.html'), await page.content()).catch(() => {});
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
  const otherChat = (await app.request(`${app.path}/chats`, member, {
    id: randomUUID(), kind: 'direct', name: 'Member private conversation', users: [], agents: [app.profile.id],
  })).value.id;
  const knowledge = `${app.path}/agent-profiles/${app.profile.id}/knowledge`;
  const notes = async (cookie = app.cookie) => (await app.request(knowledge, cookie)).value.notes;
  await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  const [name, value] = app.cookie.split('=');
  await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
  page = await context.newPage();
  const errors = [], writes = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('request', request => {
    if (request.url().endsWith(knowledge) && request.method() === 'POST') writes.push(request.postDataJSON());
  });
  const workspace = app.path.split('/').at(-1);
  await page.goto(`${app.base}/w/${workspace}/?view=agents&selected=${app.profile.id}`);
  await page.getByRole('tab', { name: 'Knowledge', exact: true }).click();
  const area = page.locator('.agent-knowledge');
  await area.getByRole('button', { name: 'Add approved note', exact: true }).click();
  const audience = area.getByRole('combobox', { name: /^Where can this note be used/ });
  assert.equal(await audience.inputValue(), '');
  assert.equal(await area.getByRole('button', { name: 'Save note', exact: true }).isDisabled(), true);
  await page.screenshot({ path: join(app.data, 'knowledge-choose-audience.png') });
  await audience.selectOption(app.conversation);
  await area.getByRole('textbox', { name: 'Title', exact: true }).fill('Private working preference');
  await area.getByRole('textbox', { name: 'Note', exact: true }).fill('OWNER_ONLY_APPROVED_NOTE_V1');
  await area.getByRole('button', { name: 'Save note', exact: true }).click();
  const privateNote = area.locator('article').filter({ hasText: 'Private working preference' });
  await privateNote.waitFor();
  let saved = (await notes())[0];
  assert.equal(saved.chat, app.conversation);
  const originalRevision = saved.revision;
  assert.ok(saved.creator_id && saved.workspace && saved.agent, 'Exercise an actual metadata-bearing read record');
  assert.equal((await notes(member)).length, 0);
  await privateNote.getByRole('button', { name: 'Edit', exact: true }).click();
  assert.equal(await audience.isDisabled(), true);
  await area.getByRole('textbox', { name: 'Note', exact: true }).fill('OWNER_ONLY_APPROVED_NOTE_V2');
  await area.getByRole('button', { name: 'Save note', exact: true }).click();
  await privateNote.getByText('OWNER_ONLY_APPROVED_NOTE_V2', { exact: true }).waitFor();
  saved = (await notes())[0];
  assert.equal(saved.revision, originalRevision + 1);
  await app.request(knowledge, app.cookie, { ...writeFields(saved), chat: null }, 400);
  await app.request(knowledge, member, { ...writeFields(saved), title: 'Unauthorized edit' }, 400);
  assert.equal((await notes())[0].revision, originalRevision + 1);
  app.pass('New notes need a deliberate audience; editing a real read record succeeds while scope changes and non-maintainer writes are denied');

  await area.getByRole('button', { name: 'Add approved note', exact: true }).click();
  await audience.selectOption('agent');
  await area.getByText('This note may be used with anyone who can talk to this agent.', { exact: false }).waitFor();
  await area.getByRole('textbox', { name: 'Title', exact: true }).fill('Approved company guidance');
  await area.getByRole('textbox', { name: 'Note', exact: true }).fill('REVIEWED_SHARED_GUIDANCE');
  await page.screenshot({ path: join(app.data, 'knowledge-shared-warning.png') });
  await area.getByRole('button', { name: 'Save note', exact: true }).click();
  await area.getByRole('button', { name: 'Add approved note', exact: true }).waitFor();
  assert.equal((await notes(member)).length, 1);
  async function ask(prompt, chat, cookie) {
    const before = app.provider.requests.length;
    const job = await app.send(prompt, chat, cookie);
    await until(async () => (await app.history(chat, cookie)).some(m => m.id === job.reply && m.state === 'complete'), 'Knowledge request did not complete', 35000);
    assert.equal(app.provider.requests.length, before + 1);
    return promptText(app.provider.requests.at(-1));
  }
  const ownerInput = await ask('Prepare my private working summary.', app.conversation, app.cookie);
  assert.ok(ownerInput.includes('OWNER_ONLY_APPROVED_NOTE_V2'));
  assert.ok(ownerInput.includes('REVIEWED_SHARED_GUIDANCE'));
  assert.ok(!ownerInput.includes('OWNER_ONLY_APPROVED_NOTE_V1'));
  const memberInput = await ask('Prepare my separate working summary.', otherChat, member);
  assert.ok(!memberInput.includes('OWNER_ONLY_APPROVED_NOTE'));
  assert.ok(memberInput.includes('REVIEWED_SHARED_GUIDANCE'));
  page.once('dialog', dialog => dialog.accept());
  await privateNote.getByRole('button', { name: 'Forget', exact: true }).click();
  await privateNote.waitFor({ state: 'detached' });
  const afterForget = await ask('Prepare another working summary.', app.conversation, app.cookie);
  assert.ok(!afterForget.includes('OWNER_ONLY_APPROVED_NOTE'));
  assert.ok(afterForget.includes('REVIEWED_SHARED_GUIDANCE'));
  assert.equal(app.sql(`SELECT forgotten FROM relay_agent_knowledge WHERE id='${saved.id}'`), 't');
  app.pass('Actual Goose input receives only its allowed approved notes; forgotten knowledge is excluded from subsequent work while its audit record remains');

  const modelCount = app.provider.requests.length;
  for (const [width, height, theme] of [[1280, 900, 'light'], [720, 640, 'light'], [390, 844, 'dark']]) {
    await page.setViewportSize({ width, height });
    await page.evaluate(theme => { document.documentElement.dataset.theme = theme; }, theme);
    await area.getByRole('button', { name: 'Add approved note', exact: true }).click();
    await audience.selectOption(app.conversation);
    await area.getByRole('textbox', { name: 'Title', exact: true }).fill('A bounded draft');
    await area.getByRole('textbox', { name: 'Note', exact: true }).fill('Private draft remains in this form.');
    await area.getByRole('button', { name: 'Save note', exact: true }).scrollIntoViewIfNeeded();
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
    await page.screenshot({ path: join(app.data, `knowledge-${width}-${theme}.png`) });
    await area.getByRole('button', { name: 'Cancel', exact: true }).click();
  }
  await page.reload();
  await area.getByText('REVIEWED_SHARED_GUIDANCE', { exact: true }).waitFor();
  assert.equal(app.provider.requests.length, modelCount);
  for (const write of writes) assert.deepEqual(Object.keys(write).sort(), Object.keys(writeFields(saved)).sort());
  assert.equal(writes.length, 4);
  assert.deepEqual(errors, []);
  await writeFile(join(app.data, 'knowledge-audience.json'), JSON.stringify({ passed: true,
    explicit_audience: true, metadata_edit_and_forget: true, real_goose_scope: true,
    forget_excluded_from_next_work: true, denied_scope_change: true, denied_member_write: true,
    model_requests: modelCount, ui_mutations: writes.length, sizes: [1280, 720, 390] }, null, 2));
  app.pass('Desktop and narrow/dark forms keep bounded scrolling; reload preserves notes and inspection produces no additional inference');
});
