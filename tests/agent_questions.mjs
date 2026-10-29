import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { remoteControlMcp } from './fixtures/remote_control_mcp.mjs';
import { until } from './fixtures/polling.mjs';
const root = resolve(import.meta.dirname, '..');
const { chromium } = createRequire(join(root, 'web/package.json'))('playwright');

test('ordinary questions release execution, survive restart, reconstruct answers and retain exact business approval', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 240000,
}, async t => {
  const remote = await remoteControlMcp(); t.after(() => remote.close());
  let app, browser; const pages = [], errors = [];
  t.after(async () => {
    try {
      if (app && !t.passed) for (const [i, page] of pages.entries()) {
        await page.screenshot({ path: join(app.data, `failure-${i}.png`), timeout: 5000 }).catch(() => {});
        await writeFile(join(app.data, `failure-${i}.html`), await page.content()).catch(() => {});
      }
      if (app) await writeFile(join(app.data, 'question-model-requests.json'), JSON.stringify(app.provider.requests, null, 2), { mode: 0o600 });
    } finally { await browser?.close(); }
  });
  const directory = await mkdtemp(join(root, '.tmp/question-profile-'));
  const profiles = join(directory, 'servers.json'), bearer = join(directory, 'bearer');
  await writeFile(profiles, '[]', { mode: 0o600 }); await writeFile(bearer, remote.token, { mode: 0o600 });
  app = await workerFixture(t, { mode: 'pooled', control: true, runSeconds: 90, leaseSeconds: 15,
    capacity: 1, workspaceLimit: 1, modelLimit: 1, apiEnv: { AYATI_MCP_SERVERS_FILE: profiles } });
  const w = app.path.split('/').at(-1), member = await app.login(1);
  const owner = (await app.request('/api/identity/session', app.cookie)).value.user;
  const person = (await app.request('/api/identity/session', member)).value.user;
  await app.request(`${app.path}/chat-grants`, app.cookie, { user: person.id, agent: app.profile.id, allowed: true });
  const channel = (await app.request(`${app.path}/chats`, app.cookie, { id: randomUUID(), kind: 'channel', name: 'Delivery coordination', users: [person.id], agents: [app.profile.id] })).value.id;
  const thread = randomUUID(); await app.request(`${app.path}/chats/${channel}/messages`, app.cookie, { id: thread, content: 'Prepare a reviewed delivery update.' });
  const connection = randomUUID(), control = `${app.path}/agent-profiles/${app.profile.id}/control`;
  await writeFile(profiles, JSON.stringify([{ id: 'orders', workspace: w, name: 'Company orders', account: 'Test company', url: remote.url, bearer_file: bearer, operations: remote.operations }]));
  await app.request(`${app.path}/control/systems`, app.cookie, { server: 'orders', approved: true, revision: 0 });
  await app.request(`${app.path}/control/connections`, app.cookie, { id: connection, server: 'orders', enabled: true, revision: 0 });
  for (const [operation, mode] of [['order.read', 'allow'], ['order.update', 'ask']]) await app.request(`${control}/rules`, app.cookie, { connection, operation, mode, revision: 0, chat: channel });
  const state = job => JSON.parse(app.sql(`SELECT row_to_json(j) FROM relay_chat_jobs j WHERE reply='${job.reply}'`));
  const questionFor = async job => until(() => {
    const row = app.sql(`SELECT row_to_json(q) FROM relay_task_questions q WHERE reply='${job.reply}' ORDER BY created_ms DESC LIMIT 1`);
    return row ? JSON.parse(row) : undefined;
  }, 'Agent did not save a question', 35000);
  const waitInput = async job => {
    await until(() => state(job).state === 'waiting', 'Question did not release its execution worker', 35000);
    const question = await questionFor(job); assert.equal(question.state, 'pending'); assert.equal(state(job).lease_until, 0); return question;
  };
  const readQuestion = (id, cookie = app.cookie) => app.request(`${app.path}/questions/${id}`, cookie);
  const formTitle = 'Delivery update details';
  const specification = { version: 1, kind: 'form', title: formTitle, description: 'Supply the missing facts. A separate approval will still be needed before changing the order.', fields: [
    { id: 'contact', label: 'Delivery contact', kind: 'text', required: true },
    { id: 'quantity', label: 'Additional quantity', kind: 'number', required: true, min: 0, max: 100, suggested: 0 },
    { id: 'date', label: 'Delivery date', kind: 'date', required: true },
    { id: 'express', label: 'Express delivery', kind: 'boolean', required: true },
    { id: 'method', label: 'Delivery method', kind: 'choice', required: true, options: [{ id: 'road', label: 'Road' }, { id: 'air', label: 'Air' }] },
    { id: 'notes', label: 'Delivery notes', kind: 'multiline' },
  ] };
  const text = 'Check the missing delivery details, then propose the order update and verify the result after review.';
  const write = { tool: 'request_action', arguments: { connection, operation: 'order.update', arguments: { order: 'one', address: 'REVIEWED_AFTER_ORDINARY_ANSWER' } } };
  app.provider.setControl(text, { steps: [{ tool: 'ask_question', arguments: { key: 'delivery-details', specification } }],
    checkpoint: 'I saved a short question for the missing delivery details. The task will continue after your answer; no order has been changed.',
    afterQuestion: [{ tool: 'operations', arguments: {} }, write],
    questionCheckpoint: 'Your delivery details are saved. The exact order update now needs its own approval; I will verify the result afterwards.',
    after: [{ tool: 'operations', arguments: {} }, write, { tool: 'request_action', arguments: { connection, operation: 'order.read', arguments: { order: 'one' } } }],
  });
  const firstWorker = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  const input = randomUUID(); await app.request(`${app.path}/chats/${channel}/messages`, app.cookie, { id: input, content: text, agent: app.profile.id, thread });
  const original = JSON.parse(app.sql(`SELECT j.payload FROM relay_chat_jobs j JOIN relay_chat_messages m ON m.id=j.reply WHERE m.chat='${channel}' AND m.thread='${thread}' ORDER BY j.sequence DESC LIMIT 1`));
  const question = await waitInput(original);
  assert.equal(remote.calls.length, 0);
  await until(() => app.sql("SELECT count(*) FROM relay_executor_slots WHERE state='ready'") === '1', 'Question retained its slot', 35000);
  const idleCalls = app.provider.requests.length;
  await new Promise(resolve => setTimeout(resolve, 1200)); assert.equal(app.provider.requests.length, idleCalls);
  const other = await app.send('Prepare an unrelated short update while the delivery question waits'); await app.done(other);
  assert.equal(state(original).state, 'waiting');
  app.pass('Real Goose saves the missing-facts form, checkpoints the same task/thread, releases its worker and permits other work without model polling');

  browser = await chromium.launch({ headless: true });
  async function pageFor(cookie) {
    const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
    const [name, value] = cookie.split('='); await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
    const page = await context.newPage(); page.on('pageerror', error => errors.push(error.message)); pages.push(page); return page;
  }
  const page = await pageFor(app.cookie), observer = await pageFor(member);
  const url = job => `${app.base}/w/${w}/?view=chat&conversation=${job.chat}&message=${job.reply}`;
  await page.goto(url(original)); await page.getByRole('button', { name: 'Answer question', exact: true }).click();
  let dialog = page.getByRole('dialog', { name: formTitle, exact: true }); await dialog.waitFor();
  await page.screenshot({ path: join(app.data, 'question-form-desktop.png') });
  await page.keyboard.press('Escape'); await dialog.waitFor({ state: 'detached' });
  assert.equal(await page.getByRole('button', { name: 'Answer question', exact: true }).evaluate(node => node === document.activeElement), true);
  assert.equal(state(original).state, 'waiting'); await page.getByRole('button', { name: 'Answer question', exact: true }).click();
  const beforeRefresh = app.provider.requests.length; await dialog.getByRole('button', { name: 'Refresh question', exact: true }).click(); assert.equal(app.provider.requests.length, beforeRefresh);
  await observer.goto(url(original)); await observer.getByRole('button', { name: 'View question', exact: true }).click();
  await observer.getByText('Only the intended requester can answer.', { exact: false }).waitFor();
  await app.request(`${app.path}/questions/${question.id}`, member, { id: randomUUID(), revision: 1, decision: 'answer', values: {} }, 400);
  assert.equal(state(original).state, 'waiting');
  await app.stop(firstWorker); await app.stopApi(); await app.startApi(); await page.reload();
  await page.getByRole('button', { name: 'Answer question', exact: true }).click();
  dialog = page.getByRole('dialog', { name: formTitle, exact: true });
  await dialog.getByLabel('Delivery contact', { exact: false }).fill('Ananya Rao');
  await dialog.getByLabel('Delivery date', { exact: false }).fill('2028-02-29');
  await dialog.getByLabel('Express delivery', { exact: false }).selectOption('false');
  await dialog.getByLabel('Delivery method', { exact: false }).selectOption('road');
  await dialog.getByLabel('Delivery notes', { exact: false }).fill('Call before arrival.');
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({ path: join(app.data, 'question-form-mobile.png') });
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
  await dialog.getByRole('button', { name: 'Submit answer', exact: true }).click();
  await dialog.getByText('Answer saved', { exact: true }).waitFor();
  assert.equal(state(original).state, 'queued'); assert.equal(state(original).run_cycle, 1); assert.equal(remote.calls.length, 0);
  const saved = JSON.parse(app.sql(`SELECT row_to_json(q) FROM relay_task_questions q WHERE id='${question.id}'`));
  assert.deepEqual(saved.answer, { contact: 'Ananya Rao', quantity: 0, date: '2028-02-29', express: false, method: 'road', notes: 'Call before arrival.' });
  await app.request(`${app.path}/questions/${question.id}`, app.cookie, { id: saved.submission, revision: 1, decision: 'answer', values: saved.answer });
  await app.request(`${app.path}/questions/${question.id}`, app.cookie, { id: randomUUID(), revision: 2, decision: 'answer', values: saved.answer }, 400);
  assert.equal(state(original).run_cycle, 1);
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  app.pass('Pending UI survives controller/browser restart; wrong respondent and stale revision are denied; the typed answer is durable and duplicate acknowledgment queues once without a business effect');

  await app.stopApi(); await app.startApi();
  const replacement = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  await until(() => state(original).state === 'waiting' && app.sql(`SELECT count(*) FROM relay_control_actions WHERE job='${original.reply}' AND state='pending'`) === '1', 'Ordinary answer did not continue to a separate exact approval', 35000);
  assert.equal(remote.calls.filter(call => call.name === 'update_order').length, 0);
  await page.setViewportSize({ width: 1280, height: 900 }); await page.goto(url(original));
  await page.getByRole('button', { name: 'Review saved request', exact: true }).click();
  const approval = page.getByRole('dialog', { name: 'Review approval request', exact: true });
  await approval.getByRole('button', { name: 'Approve this action', exact: true }).click();
  await approval.getByRole('button', { name: 'Close', exact: true }).click();
  const completed = await app.done(original);
  assert.match(completed.content, /REVIEWED_AFTER_ORDINARY_ANSWER/); assert.match(completed.content, /Ananya Rao/);
  assert.equal(remote.calls.filter(call => call.name === 'update_order').length, 1); assert.equal(remote.calls.filter(call => call.name === 'read_order').length, 1);
  assert.equal(state(original).run_cycle, 2); assert.equal(state(original).payload.prompt, original.prompt);
  await page.reload(); await page.getByRole('button', { name: 'View question', exact: true }).click();
  dialog = page.getByRole('dialog', { name: formTitle, exact: true }); await dialog.getByText('Answer saved', { exact: true }).waitFor();
  await page.evaluate(() => { document.documentElement.dataset.theme = 'dark'; });
  await page.screenshot({ path: join(app.data, 'question-saved-dark.png') }); await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  app.pass('Fresh execution reconstructs the same task with facts, obtains independent exact action approval, writes once and verifies once; saved answers remain readable after completion');

  const multiText = 'Choose the methods to include in the delivery comparison.';
  app.provider.setControl(multiText, { steps: [{ tool: 'ask_question', arguments: { key: 'methods', specification: { version: 1, kind: 'multiple_choice', title: 'Comparison methods', options: [{ id: 'road', label: 'Road' }, { id: 'air', label: 'Air' }], allow_other: true, min_choices: 1, max_choices: 2 } } }], checkpoint: 'Choose which delivery methods to compare.' });
  const multi = await app.send(multiText, channel); await waitInput(multi); await page.goto(url(multi));
  await page.getByRole('button', { name: 'Answer question', exact: true }).click(); dialog = page.getByRole('dialog', { name: 'Comparison methods', exact: true });
  await dialog.getByRole('checkbox', { name: 'Road', exact: true }).check(); await dialog.getByRole('checkbox', { name: 'Another answer', exact: true }).check();
  await dialog.getByRole('textbox', { name: 'Another answer', exact: true }).fill('Train'); await dialog.getByRole('button', { name: 'Submit answer', exact: true }).click();
  assert.match((await app.done(multi)).content, /Train/);
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  app.pass('Multiple choice uses stable option IDs and an explicitly enabled text alternative; selecting controls performs no business operation');

  const privateAgent = { id: randomUUID(), name: 'Private planning helper', role: 'Clarify plans privately.', model: 'fixture', scope: 'private', revision: 0, settings: { provider: 'openai' } };
  await app.request(`${app.path}/agent-profiles`, app.cookie, privateAgent);
  const privateChat = (await app.request(`${app.path}/chats`, app.cookie, { id: randomUUID(), kind: 'direct', name: privateAgent.name, users: [], agents: [privateAgent.id] })).value.id;
  const closed = text => ({ steps: [{ tool: 'ask_question', arguments: { key: 'decision', specification: { version: 1, kind: 'single_choice', title: text, options: [{ id: 'yes', label: 'Yes' }, { id: 'no', label: 'No' }] } } }], checkpoint: 'A private clarification is saved. No business operation has been requested.' });
  const privateText = 'Should the private plan include express delivery?'; app.provider.setControl(privateText, closed('Include express delivery?'));
  const personal = await app.send(privateText, privateChat, app.cookie, privateAgent.id); const privateQuestion = await waitInput(personal);
  await app.request(`${app.path}/questions/${privateQuestion.id}`, member, undefined, 400);
  await page.setViewportSize({ width: 390, height: 844 }); await page.goto(url(personal)); await page.getByRole('button', { name: 'Answer question', exact: true }).click();
  dialog = page.getByRole('dialog', { name: 'Include express delivery?', exact: true });
  assert.equal(await dialog.getByRole('textbox', { name: 'Another answer', exact: true }).count(), 0);
  await dialog.getByRole('button', { name: 'Close', exact: true }).click(); assert.equal(state(personal).state, 'waiting');
  await page.getByRole('button', { name: 'Answer question', exact: true }).click();
  await dialog.getByRole('radio', { name: 'Yes', exact: true }).focus(); await page.keyboard.press('ArrowRight');
  assert.equal(await dialog.getByRole('radio', { name: 'No', exact: true }).isChecked(), true);
  await page.setViewportSize({ width: 320, height: 600 });
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
  await page.screenshot({ path: join(app.data, 'question-closed-choice-mobile.png') });
  await dialog.getByRole('button', { name: 'Submit answer', exact: true }).click(); assert.match((await app.done(personal)).content, /"choices":\["no"\]/);
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();

  const declineText = 'Ask which optional topic to investigate next.'; app.provider.setControl(declineText, closed('Choose an optional topic'));
  const declined = await app.send(declineText, privateChat, app.cookie, privateAgent.id); const declineQuestion = await waitInput(declined); await page.goto(url(declined));
  await page.getByRole('button', { name: 'Answer question', exact: true }).click(); dialog = page.getByRole('dialog', { name: 'Choose an optional topic', exact: true });
  await dialog.getByRole('button', { name: 'Decline question', exact: true }).click(); assert.match((await app.done(declined)).content, /declined/);
  assert.equal((await readQuestion(declineQuestion.id)).value.state, 'declined'); await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  const stopText = 'Ask a clarification that I may stop.'; app.provider.setControl(stopText, closed('Clarification before work'));
  const stopped = await app.send(stopText, privateChat, app.cookie, privateAgent.id); const stopQuestion = await waitInput(stopped); await page.goto(url(stopped));
  await page.getByRole('button', { name: 'Stop reply', exact: true }).click(); await app.done(stopped, 'cancelled');
  assert.equal((await readQuestion(stopQuestion.id)).value.state, 'invalidated');
  await app.request(`${app.path}/questions/${stopQuestion.id}`, app.cookie, { id: randomUUID(), revision: 1, decision: 'answer', values: { choices: ['yes'] } }, 400);
  assert.equal(remote.calls.filter(call => call.name === 'update_order').length, 1);
  app.pass('Managed private agents without business connections ask the same controls; No, dismiss, decline and Stop retain distinct saved outcomes and private access stays enforced');

  const revokedText = 'Ask for one source-scoped fact before continuing.';
  app.provider.setControl(revokedText, closed('Source-scoped clarification'));
  const revoked = await app.send(revokedText, channel, member); const revokedQuestion = await waitInput(revoked);
  await observer.goto(url(revoked)); await observer.getByRole('button', { name: 'Answer question', exact: true }).click();
  const revokedDialog = observer.getByRole('dialog', { name: 'Source-scoped clarification', exact: true });
  await revokedDialog.getByRole('radio', { name: 'Yes', exact: true }).check();
  await app.request(`${app.path}/chats/${channel}/members`, app.cookie, { users: [owner.id] });
  await revokedDialog.waitFor({ state: 'detached' }); await app.done(revoked, 'cancelled');
  await app.request(`${app.path}/questions/${revokedQuestion.id}`, member, undefined, 400);
  await app.request(`${app.path}/chats/${channel}/members`, app.cookie, { users: [owner.id, person.id] });
  assert.equal((await readQuestion(revokedQuestion.id, member)).value.state, 'invalidated');
  await app.request(`${app.path}/questions/${revokedQuestion.id}`, member, { id: randomUUID(), revision: 1, decision: 'answer', values: { choices: ['yes'] } }, 400);
  assert.equal(state(revoked).state, 'finished');
  app.pass('Source revocation clears the open input and cancels the task; restoring membership cannot revive or answer the invalidated question');

  await app.stop(replacement);
  assert.deepEqual(errors, []); assert.equal(JSON.stringify(app.provider.requests).includes(remote.token), false);
  assert.equal(app.processes().some(process => process.logs.includes(remote.token)), false);
});

test('controller-local broker adapter supports durable questions without business connections or remote gateway', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 90000,
}, async t => {
  // Simulate intentionally keeps embedded replies deterministic. Exercise the
  // same local-broker/execute_bounded path in an isolated compatibility worker.
  const app = await workerFixture(t, { mode: 'external', runSeconds: 30 });
  const worker = await app.startWorker({ AYATI_CHAT_EXECUTION: 'embedded' });
  const prompt = 'Ask whether to include the optional summary.';
  app.provider.setControl(prompt, { steps: [{ tool: 'ask_question', arguments: { key: 'include-summary', specification: {
    version: 1, kind: 'single_choice', title: 'Include the optional summary?', options: [{ id: 'yes', label: 'Yes' }, { id: 'no', label: 'No' }],
  } } }], checkpoint: 'The optional summary question is saved.' });
  const task = await app.send(prompt);
  await until(() => app.sql(`SELECT state FROM relay_chat_jobs WHERE reply='${task.reply}'`) === 'waiting', 'Embedded question did not release execution', 35000);
  const question = JSON.parse(app.sql(`SELECT row_to_json(q) FROM relay_task_questions q WHERE reply='${task.reply}'`));
  const modelCalls = app.provider.requests.length;
  await new Promise(resolve => setTimeout(resolve, 600)); assert.equal(app.provider.requests.length, modelCalls);
  await app.request(`${app.path}/questions/${question.id}`, app.cookie, { id: randomUUID(), revision: 1, decision: 'answer', values: { choices: ['yes'] } });
  assert.match((await app.done(task)).content, /"choices":\["yes"\]/);
  assert.equal(app.sql(`SELECT count(*) FROM relay_control_connections`), '0');
  assert.equal(app.sql(`SELECT count(*) FROM relay_task_checkpoints WHERE reply='${task.reply}'`), '1');
  await app.stop(worker);
  app.pass('The controller-local broker execution adapter asks and reconstructs without configured business tools, a remote control gateway or idle model polling; exercised in an isolated compatibility worker');
});
