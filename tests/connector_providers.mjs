// Connectors through a provider, with real Goose: an admin approves Gmail and
// connects it through the provider's sign-in page from Settings, an agent sends
// an email through Cuckoo's gateway under Cuckoo's own action name, Cuckoo can
// switch the provider off at once, and an expired account asks to be
// reconnected. The provider here is a local stand-in for Composio.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { composioProvider, COMPOSIO_KEY, COMPOSIO_TOKEN } from './fixtures/composio_provider.mjs';
import { until } from './fixtures/polling.mjs';
const root = resolve(import.meta.dirname, '..');
const { chromium } = createRequire(join(root, 'web/package.json'))('playwright');

test('an admin connects Gmail through its provider, an agent sends an email, and the provider can be switched off', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 180000,
}, async t => {
  const composio = await composioProvider(); t.after(() => composio.close());
  const directory = await mkdtemp(join(root, '.tmp/connector-providers-'));
  const key = join(directory, 'composio-key'); await writeFile(key, COMPOSIO_KEY, { mode: 0o600 });
  const app = await workerFixture(t, { mode: 'pooled', control: true, runSeconds: 90, leaseSeconds: 15,
    apiEnv: { AYATI_COMPOSIO_KEY_FILE: key, AYATI_COMPOSIO_BASE_URL: composio.url } });
  const w = app.path.split('/').at(-1);
  const browser = await chromium.launch({ headless: true }); const errors = [];
  const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  const [name, value] = app.cookie.split('=');
  await context.addCookies([{ name, value, url: app.base, httpOnly: true, sameSite: 'Lax' }]);
  const page = await context.newPage(); page.on('pageerror', e => errors.push(e.message));
  t.after(async () => { if (!t.passed) await page.screenshot({ path: join(app.data, 'failure.png') }).catch(() => {}); await browser.close(); });

  // Settings, Connections: approve Gmail, then connect through the sign-in page.
  await page.goto(`${app.base}/w/${w}/?view=settings&section=connections`);
  const gmail = page.getByRole('article', { name: 'Gmail' });
  await gmail.getByText('Signs in through a secure page').waitFor();
  await gmail.getByRole('button', { name: 'Approve system' }).click();
  await gmail.getByRole('button', { name: 'Connect account' }).click();
  await page.waitForURL(/connected=1/);
  await page.getByText('Account connected.', { exact: false }).waitFor();
  await gmail.getByText(/Approved · Connected/).waitFor();
  await page.screenshot({ path: join(app.data, 'gmail-connected.png') });
  const snapshot = async () => (await app.request(`${app.path}/control/connections`, app.cookie)).value;
  let connection = (await snapshot()).connections.find(c => c.server === 'gmail');
  assert.equal(connection.provider, 'composio');
  assert.deepEqual(connection.definition.operations.map(o => o.id), ['gmail.find_emails', 'gmail.draft_email', 'gmail.send_email', 'gmail.reply_to_thread']);
  app.pass('Settings lists Gmail from the provider; approving and signing in through its page connects it and comes back to Settings');

  // The admin lets this connector send without asking; an agent switches it on.
  const rule = async (mode) => {
    const safety = (await snapshot()).safety.find(s => s.connection === connection.id).operations.find(o => o.id === 'gmail.send_email');
    await app.request(`${app.path}/control/connector-rules`, app.cookie, { connection: connection.id, operation: 'gmail.send_email', mode, needs_other: false, revision: safety.revision });
  };
  await rule('allow');
  const agent = { id: randomUUID(), name: 'Nova', role: 'Accounts', model: 'fixture', audience: 'everyone', revision: 0, settings: { provider: 'openai', instructions: 'Send account emails when asked.' } };
  await app.request(`${app.path}/agent-profiles`, app.cookie, agent);
  const control = `${app.path}/agent-profiles/${agent.id}/control`;
  await app.request(`${control}/tools`, app.cookie, { connection: connection.id, enabled: true });
  const chat = (await app.request(`${app.path}/chats`, app.cookie, { id: randomUUID(), name: 'Accounts', kind: 'group', users: [], agents: [agent.id] })).value.id;
  const worker = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  const send = async (subject) => {
    const command = `control ${JSON.stringify({ tool: 'request_action', arguments: { connection: connection.id, operation: 'gmail.send_email', arguments: { to: 'sharma@example.test', subject, body: 'Your balance is due.' } } })}`;
    const job = await app.send(command, chat, app.cookie, agent.id);
    await until(async () => (await app.history(chat, app.cookie)).find(m => m.id === job.reply && m.state === 'complete'), 'Agent did not finish', 35000);
    return (await app.request(control, app.cookie)).value.actions.find(a => a.job === job.reply);
  };
  const sent = await send('September balance');
  assert.equal(sent.state, 'completed');
  assert.deepEqual(composio.executed, [{ tool: 'GMAIL_SEND_EMAIL', arguments: { recipient_email: 'sharma@example.test', subject: 'September balance', body: 'Your balance is due.' } }]);
  const asked = app.provider.requests.find(r => JSON.stringify(r.messages).includes('September balance'));
  assert.ok(!JSON.stringify(app.provider.requests).includes('GMAIL_SEND_EMAIL'), 'the model never sees the provider');
  assert.ok(asked);
  app.pass('Real Goose sent the email through the gateway under Cuckoo\'s action name; the provider got its own field names');

  // Cuckoo switches the provider off: nothing is sent and the reason is recorded.
  await app.request('/api/identity/admin/connector-providers', app.cookie, { provider: 'composio', enabled: false, note: 'Provider incident' });
  const held = await send('October balance');
  assert.equal(held.state, 'invalidated');
  assert.equal(composio.executed.length, 1);
  await app.request('/api/identity/admin/connector-providers', app.cookie, { provider: 'composio', enabled: true });
  app.pass('Switching the provider off stopped the next send before it reached the provider');

  // The account expires: the next send asks for a reconnect, and Settings offers it.
  composio.expire();
  const expired = await send('November balance');
  assert.equal(expired.state, 'invalidated');
  assert.equal(composio.executed.length, 1);
  const old = connection;
  await page.goto(`${app.base}/w/${w}/?view=settings&section=connections`);
  await gmail.getByText(/Needs reconnecting/).waitFor();
  await page.screenshot({ path: join(app.data, 'gmail-needs-reconnect.png') });
  await gmail.getByRole('button', { name: 'Reconnect', exact: true }).click();
  await page.waitForURL(/connected=1/);
  await gmail.getByText(/Approved · Connected/).waitFor();
  connection = (await snapshot()).connections.find(c => c.server === 'gmail');
  assert.equal(connection.needs_reconnect, false);
  assert.equal(composio.removed.length, 1, 'the replaced account was revoked at the provider');
  assert.equal(connection.id, old.id);
  app.pass('An expired account sent nothing, Settings showed "Needs reconnecting", and reconnecting replaced and revoked the old account');

  // Disconnecting revokes at the provider. No token reached the model, logs or database.
  await gmail.getByRole('button', { name: 'Disconnect', exact: true }).click();
  await page.getByText('Disconnected.', { exact: false }).waitFor();
  assert.equal(composio.removed.length, 2);
  assert.equal(app.sql(`SELECT count(*) FROM relay_control_connections WHERE binding::text LIKE '%${COMPOSIO_TOKEN}%' OR definition::text LIKE '%${COMPOSIO_TOKEN}%'`), '0');
  assert.equal(JSON.stringify(app.provider.requests).includes(COMPOSIO_TOKEN), false);
  assert.equal(app.processes().some(p => p.logs.includes(COMPOSIO_TOKEN) || p.logs.includes(COMPOSIO_KEY)), false);
  assert.deepEqual(errors, []);
  await app.stop(worker);
  app.pass('Disconnecting revoked the account at the provider; no provider token or key reached the model, logs or database');
});
