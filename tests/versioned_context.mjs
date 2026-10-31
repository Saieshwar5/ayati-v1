import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { writeFile } from 'node:fs/promises';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { controllerToolsOnly } from './fixtures/worker_control.mjs';
import { root, until } from './fixtures/polling.mjs';

const { chromium } = createRequire(join(root, 'web/package.json'))('playwright');
test('saved context, working summary, corrections and privacy survive real controller and Goose execution', {
  timeout: 240000, skip: !process.env.RELAY_TEST_DATABASE_URL,
}, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true, leaseSeconds: 15 });
  await app.request(`${app.path}/agent-profiles`, app.cookie, {
    ...app.profile, revision: 1, settings: { ...app.profile.settings, memory: true },
  });
  const knowledge = `${app.path}/agent-profiles/${app.profile.id}/knowledge`;
  const note = { id: randomUUID(), title: 'My approved salary', content: 'PRIVATE_SALARY_48000',
    chat: app.conversation, source_message: null, revision: 0, forgotten: false };
  await app.request(knowledge, app.cookie, note);
  const evidenceId = randomUUID();
  await app.request(`${app.path}/chats/${app.conversation}/messages`, app.cookie, {
    id: evidenceId, content: 'PRIVATE_START_DATE November 4',
  });
  const member = await app.login(1);
  const memberId = (await app.request('/api/identity/session', member)).value.user.id;
  await app.request(`${app.path}/chat-grants`, app.cookie, { user: memberId, agent: app.profile.id, allowed: true });
  const otherChat = (await app.request(`${app.path}/chats`, member, {
    id: randomUUID(), name: 'Other customer', kind: 'direct', users: [], agents: [app.profile.id],
  })).value.id;
  const command = 'Summarize my own onboarding work with evidence.';
  app.provider.setControl(command, { steps: [
    { tool: 'read_conversation_history', arguments: { message: evidenceId } },
    { tool: 'save_working_context', arguments: { revision: 0,
      objective: { text: 'MY_PRIVATE_ONBOARDING', sources: [evidenceId] },
      decisions: [{ text: 'PRIVATE_SALARY_48000', sources: [note.id] }],
      questions: [], corrections: [],
    } },
  ] });
  let executor = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  const task = await app.send(command);
  await app.done(task);
  controllerToolsOnly(app.provider.requests[0]);
  const endpoint = `${app.path}/agent-profiles/${app.profile.id}/context-preview`;
  const preview = () => app.request(`${endpoint}?chat=${app.conversation}`, app.cookie).then(r => r.value);
  const saved = await preview();
  assert.equal(saved.working.revision, 1);
  assert.ok(!JSON.stringify(saved.attempts).includes('PRIVATE_SALARY'));
  const attempt = saved.attempts[0];
  const detailUrl = `${endpoint}?chat=${app.conversation}&task=${attempt.task}&claim=${attempt.claim}`;
  const detail = (await app.request(detailUrl, app.cookie)).value;
  assert.equal(detail.reads.length, 1);
  assert.match(JSON.stringify(detail.evidence), /PRIVATE_SALARY_48000/);
  await app.request(detailUrl, member, undefined, 400);
  await app.request(`${endpoint}?chat=${otherChat}&task=${attempt.task}&claim=${attempt.claim}`, member, undefined, 400);
  assert.equal((await app.request(`${endpoint}?chat=${otherChat}`, member)).value.working.revision, 0);
  app.pass('Real Goose saves a cited summary and history receipt; task metadata is compact and another customer cannot inspect the snapshot');

  await app.stop(executor);
  await app.stopApi(); await app.startApi();
  assert.equal((await preview()).working.revision, 1);
  assert.deepEqual((await app.request(detailUrl, app.cookie)).value, detail);
  executor = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  const before = app.provider.requests.length;
  await app.done(await app.send('Continue my onboarding review.'));
  assert.match(JSON.stringify(app.provider.requests[before].messages), /MY_PRIVATE_ONBOARDING/);
  app.pass('Controller and executor restart retain the exact snapshot and carry the working summary into the next request');

  let browser;
  t.after(async () => browser?.close());
  browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  const [name, value] = app.cookie.split('=');
  await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
  const page = await context.newPage();
  const errors = []; page.on('pageerror', error => errors.push(error.message));
  await page.goto(`${app.base}/w/${app.path.split('/').at(-1)}/?view=agents&selected=${app.profile.id}`);
  await page.getByRole('tab', { name: 'Instructions', exact: true }).click();
  await page.getByRole('button', { name: 'Preview conversation context', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Conversation context preview', exact: true });
  await dialog.getByRole('combobox').selectOption(app.conversation);
  await dialog.getByRole('heading', { name: 'Working summary · Revision 1', exact: true }).waitFor();
  await dialog.getByRole('button', { name: 'View saved evidence', exact: true }).last().click();
  await dialog.getByRole('heading', { name: 'Later history reads', exact: true }).waitFor();
  await page.screenshot({ path: join(app.data, 'saved-context-desktop.png') });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.evaluate(() => { document.documentElement.dataset.theme = 'dark'; });
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
  await page.screenshot({ path: join(app.data, 'saved-context-phone-dark.png') });

  app.provider.setHeld(true);
  const beforeHeld = app.provider.requests.length;
  const inFlight = await app.send('Work while my salary note is corrected.');
  await until(() => app.provider.requests.length > beforeHeld, 'Context task did not start');
  await app.request(knowledge, app.cookie, { ...note, content: 'CORRECTED_SALARY_52000' });
  await app.done(inFlight, 'cancelled');
  app.provider.setHeld(false);
  await dialog.getByRole('button', { name: 'Refresh preview', exact: true }).click();
  await dialog.getByText('Outdated or unavailable. New work uses fresh evidence.', { exact: true }).waitFor();
  assert.ok(!(await dialog.innerText()).includes('PRIVATE_SALARY_48000'));
  const outdated = (await app.request(detailUrl, app.cookie)).value;
  assert.equal(outdated.available, false);
  assert.ok(!JSON.stringify(outdated).includes('PRIVATE_SALARY_48000'));
  const after = app.provider.requests.length;
  await app.done(await app.send('Use only the corrected approved salary.'));
  const fresh = JSON.stringify(app.provider.requests[after].messages);
  assert.match(fresh, /CORRECTED_SALARY_52000/);
  assert.ok(!fresh.includes('PRIVATE_SALARY_48000'));
  await app.request(knowledge, app.cookie, { ...note, revision: 1, content: 'CORRECTED_SALARY_52000', forgotten: true });
  const forgotten = await preview();
  assert.ok(!JSON.stringify(forgotten).includes('CORRECTED_SALARY_52000'));
  assert.deepEqual(errors, []);
  await app.stop(executor);
  app.pass('Live desktop and phone UI inspect evidence; correction cancels in-flight work, hides old summaries and answers, and forgetting removes derived context from future retrieval');
  await writeFile(join(app.data, 'versioned-context.json'), JSON.stringify({ passed: true,
    restart: true, private_audience: true, history_receipts: true, correction: true,
    forgotten: true, stale_publication_blocked: true, browser_errors: errors }, null, 2));
});
