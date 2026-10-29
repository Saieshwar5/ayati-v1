import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { createRequire } from 'node:module';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { remoteControlMcp } from './fixtures/remote_control_mcp.mjs';
import { restoredWorkspace } from './fixtures/restored_workspace.mjs';
import { root, until } from './fixtures/polling.mjs';

const { chromium } = createRequire(join(root, 'web/package.json'))('playwright');

test('a quiesced workspace restore preserves waiting work, reviewed Home and private files without replay', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 240000,
}, async t => {
  let browser, page, restoreCleanup;
  t.after(async () => {
    try {
      if (!t.passed && page) await page.screenshot({ path: join(directory, 'failure.png') }).catch(() => {});
      await browser?.close();
    } finally { await restoreCleanup?.(); }
  });
  const remote = await remoteControlMcp();
  t.after(() => remote.close());
  const directory = await mkdtemp(join(root, '.tmp/restore-profile-'));
  const profiles = join(directory, 'servers.json'), bearer = join(directory, 'bearer');
  await writeFile(profiles, '[]', { mode: 0o600 });
  await writeFile(bearer, remote.token, { mode: 0o600 });
  const app = await workerFixture(t, { mode: 'pooled', control: true, capacity: 1,
    workspaceLimit: 1, modelLimit: 1, leaseSeconds: 15,
    apiEnv: { AYATI_MCP_SERVERS_FILE: profiles } });
  const workspace = app.path.split('/').at(-1), connection = randomUUID();
  await writeFile(profiles, JSON.stringify([{ id: 'orders', workspace, name: 'Local orders',
    account: 'Synthetic company', url: remote.url, bearer_file: bearer, operations: remote.operations }]));
  await app.request(`${app.path}/control/systems`, app.cookie, { server: 'orders', approved: true, revision: 0 });
  await app.request(`${app.path}/control/connections`, app.cookie, { id: connection, server: 'orders', enabled: true, revision: 0 });
  const control = `${app.path}/agent-profiles/${app.profile.id}/control`;
  await app.request(`${control}/rules`, app.cookie, {
    connection, operation: 'order.update', mode: 'ask', revision: 0, chat: app.conversation,
  });
  const worker = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  const panel = randomUUID(), panelPrompt = 'Save my recovery priorities panel.';
  app.provider.setControl(panelPrompt, { steps: [{ tool: 'save_home_panel', arguments: {
    id: panel, expected_revision: 0, title: 'Recovery priorities', purpose: 'A personal recovery check',
    template: 'brief', sections: [{ kind: 'text', heading: 'Next step', body: 'Check the pending delivery question.' }], sources: [],
  } }], checkpoint: 'Your personal draft is saved for review.' });
  await app.done(await app.send(panelPrompt));
  const panels = `${app.path}/home-panels`;
  await app.request(`${panels}/${panel}/review`, app.cookie, { request: randomUUID(), revision: 1 });
  await app.request(`${panels}/layout`, app.cookie, {
    request: randomUUID(), revision: 0, order: [panel], wide: [panel], hidden: [],
  });
  const homeBefore = (await app.request(panels, app.cookie)).value;

  const prompt = 'Ask me whether to include a delivery summary.';
  app.provider.setControl(prompt, { steps: [{ tool: 'ask_question', arguments: {
    key: 'summary', specification: { version: 1, kind: 'single_choice', title: 'Include a summary?',
      options: [{ id: 'yes', label: 'Yes' }, { id: 'no', label: 'No' }] },
  } }], checkpoint: 'Your delivery question is saved. I will continue after your answer.' });
  const questionTask = await app.send(prompt);
  await until(() => app.sql(`SELECT state FROM relay_chat_jobs WHERE reply='${questionTask.reply}'`) === 'waiting', 'Question did not wait', 35000);
  const question = JSON.parse(app.sql(`SELECT row_to_json(q) FROM relay_task_questions q WHERE reply='${questionTask.reply}'`));
  const questionBefore = (await app.request(`${app.path}/questions/${question.id}`, app.cookie)).value;

  const update = `control ${JSON.stringify({ tool: 'request_action', arguments: {
    connection, operation: 'order.update', arguments: { order: 'lost', address: 'APPLIED_BEFORE_BACKUP' },
  }, checkpoint: 'The address change needs your approval.' })}`;
  const uncertain = await app.send(update);
  const snapshot = async () => (await app.request(control, app.cookie)).value;
  const action = await until(async () => (await snapshot()).actions.find(a => a.job === uncertain.reply), 'No approval saved', 35000);
  await app.request(`${control}/actions/${action.id}/decision`, app.cookie, { id: randomUUID(), decision: 'approve' });
  assert.equal((await snapshot()).actions.find(a => a.id === action.id).state, 'uncertain');
  assert.equal(remote.records.get('lost').address, 'APPLIED_BEFORE_BACKUP');
  assert.equal(remote.calls.length, 1);

  const bytes = Buffer.from('PRIVATE_RESTORE_ATTACHMENT\nOnly the original direct conversation can read this.\n');
  const upload = await fetch(`${app.base}${app.path}/chats/${app.conversation}/objects?name=recovery.txt`, {
    method: 'POST', headers: { Cookie: app.cookie, Origin: app.base, 'X-Ayati-Request': 'identity', 'Content-Type': 'text/plain' }, body: bytes,
  });
  assert.equal(upload.status, 200, await upload.clone().text());
  const object = await upload.json();
  await app.request(`${app.path}/chats/${app.conversation}/messages`, app.cookie, {
    id: randomUUID(), content: 'Private recovery reference', attachments: [object.id],
  });
  await app.stop(worker);
  await app.stopApi();
  const sourceBefore = app.sql(`SELECT row_to_json(j) FROM relay_chat_jobs j WHERE reply='${uncertain.reply}'`);
  const beforeRequests = app.provider.requests.length;
  const restored = await restoredWorkspace({ after: fn => { restoreCleanup = fn; }, get passed() { return t.passed; } }, app, profiles);
  assert.equal(restored.report.objects, 1);
  const owner = await app.login(0), other = await app.login(1);
  assert.deepEqual((await app.request(panels, owner)).value, homeBefore);
  assert.equal((await app.request(panels, other)).value.panels.length, 0);
  assert.deepEqual((await app.request(`${app.path}/questions/${question.id}`, owner)).value, questionBefore);
  await app.request(`${app.path}/questions/${question.id}`, other, undefined, 400);
  assert.equal((await app.request(control, owner)).value.actions.find(a => a.id === action.id).state, 'uncertain');
  const file = app.base + `${app.path}/objects/${object.id}`;
  const download = await fetch(file, { headers: { Cookie: owner } });
  assert.equal(download.status, 200);
  assert.deepEqual(Buffer.from(await download.arrayBuffer()), bytes);
  assert.equal((await fetch(file, { headers: { Cookie: other } })).status, 400);
  app.pass('Restored pending question, uncertain action, exact reviewed Home/layout and private file retain their identity and access boundaries');

  await restored.startExecutor();
  await until(() => Number(restored.sql("SELECT count(*) FROM relay_executors WHERE state='serving' AND heartbeat > extract(epoch from clock_timestamp())::bigint - 3")) > 0,
    'Restored executor did not register');
  browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  const [name, value] = owner.split('=');
  await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
  page = await context.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto(`${app.base}/w/${workspace}/?view=home`);
  await page.locator(`[data-panel="${panel}"]`).waitFor();
  await page.screenshot({ path: join(restored.output, 'restored-home.png') });
  await page.goto(`${app.base}/w/${workspace}/?view=chat&conversation=${app.conversation}`);
  await page.getByRole('button', { name: 'Answer question', exact: true }).waitFor();
  await page.getByRole('status').filter({ hasText: 'Needs attention · External outcome not yet confirmed' }).waitFor();
  await page.screenshot({ path: join(restored.output, 'restored-waiting-work.png') });
  assert.equal(app.provider.requests.length, beforeRequests);
  assert.equal(remote.calls.length, 1);
  assert.equal(restored.sql(`SELECT state FROM relay_chat_jobs WHERE reply='${questionTask.reply}'`), 'waiting');
  assert.equal(restored.sql(`SELECT state FROM relay_chat_jobs WHERE reply='${uncertain.reply}'`), 'waiting');
  assert.equal(app.sql(`SELECT row_to_json(j) FROM relay_chat_jobs j WHERE reply='${uncertain.reply}'`), sourceBefore);
  assert.deepEqual(errors, []);
  const manifest = JSON.parse(await readFile(join(restored.output, 'manifest.json'), 'utf8'));
  await writeFile(join(restored.output, 'behavior-evidence.json'), JSON.stringify({
    passed: true, source_schema: restored.report.schema, tables: Object.keys(manifest.tables).length,
    question: question.id, uncertain_action: action.id, panel, object: object.id,
    model_requests_before: beforeRequests, model_requests_after: app.provider.requests.length,
    external_effects_before: 1, external_effects_after: remote.calls.length,
    screenshot_paths: ['restored-home.png', 'restored-waiting-work.png'],
  }, null, 2), { mode: 0o600 });
  app.pass('Restored controller and executor display waiting work without another inference or repeated external write; source remains unchanged');
});
