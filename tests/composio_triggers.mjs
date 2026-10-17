import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHmac, randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { mkdir } from 'node:fs/promises';
import { request as httpRequest } from 'node:http';
import { setup, until, root } from './fixtures/goose_app.mjs';
import { composioFixture } from './fixtures/composio.mjs';
const { chromium } = createRequire(join(root, 'web/package.json'))('playwright');
const secret = 'fixture-webhook-secret';
function wireRequest(url, method, headers, body = '') {
  return new Promise((resolve, reject) => {
    const request = httpRequest(url, { method, headers: { ...headers, Connection: 'close', 'Content-Length': Buffer.byteLength(body) } }, response => {
      response.resume(); response.on('end', () => resolve({ status: response.statusCode }));
    });
    request.on('error', reject); request.end(body);
  });
}

test('signed Composio events reach only saved rules and do not replay after restart', { timeout: 60000 }, async t => {
  const provider = await composioFixture(); t.after(provider.close);
  const app = await setup(t, false, { COMPOSIO_API_KEY: 'fixture-composio-key', AYATI_COMPOSIO_BASE_URL: provider.url, COMPOSIO_WEBHOOK_SECRET: secret });
  const connection = await app.request('/connections', { toolkit: 'gmail', auth_config_id: 'ac_fixture', label: 'Event Gmail' });
  await app.request(`/connections/${connection.id}/refresh`, {});
  const input = { id: randomUUID(), connection_id: connection.id, name: 'Stock report', slug: 'GMAIL_NEW_GMAIL_MESSAGE', provider_id: 'ti_fixture', instructions: 'Summarize the new stock message', filter: { kind: 'stock' } };
  await app.request('/triggers', input); await app.request('/triggers', input);
  const user = provider.calls.find(c => c.path.endsWith('/link')).body.user_id;
  const payload = (id, kind = 'stock', override = {}) => ({ id, type: 'composio.trigger.message', metadata: { user_id: user, connected_account_id: 'ca_fixture', trigger_id: 'ti_fixture', trigger_slug: input.slug, ...override }, data: { kind, text: 'Synthetic email text' } });
  async function deliver(event, options = {}) {
    const body = JSON.stringify(event), timestamp = String(options.time ?? Math.floor(Date.now() / 1000));
    const signature = createHmac('sha256', options.secret ?? secret).update(`${event.id}.${timestamp}.${body}`).digest('base64');
    return wireRequest(app.base.replace('/api', '/hooks/composio'), 'POST', { Host: 'public.example', Origin: 'https://public.example', 'Content-Type': 'application/json', 'webhook-id': event.id, 'webhook-timestamp': timestamp, 'webhook-signature': `v1,${signature}` }, body);
  }
  assert.equal((await deliver(payload('msg_bad'), { secret: 'wrong-secret' })).status, 401);
  assert.equal((await deliver(payload('msg_old'), { time: Math.floor(Date.now() / 1000) - 600 })).status, 401);
  assert.equal((await deliver(payload('msg_foreign', 'stock', { user_id: 'foreign' }))).status, 503);
  assert.equal((await deliver(payload('msg_unmatched', 'other'))).status, 200);
  assert.equal((await app.state()).tasks.length, 0);
  assert.equal((await deliver(payload('msg_stock'))).status, 200);
  const run = await until(async () => (await app.request(`/triggers/${input.id}/runs`)).runs[0]?.task_id, 'Event task not accepted');
  assert.equal((await app.done(run)).task.status, 'completed');
  assert.equal((await deliver(payload('msg_stock'))).status, 200);
  assert.equal((await app.state()).tasks.length, 1);
  await app.request(`/triggers/${input.id}/state`, { state: 'paused', expected_revision: 1 });
  assert.equal((await deliver(payload('msg_paused'))).status, 200);
  await app.stop(); await app.start();
  assert.equal((await app.request('/triggers')).triggers[0].state, 'paused');
  await app.request(`/triggers/${input.id}/state`, { state: 'active', expected_revision: 2 });
  assert.equal((await deliver(payload('msg_paused'))).status, 200, 'Old unmatched receipt is not rematched after resume');
  assert.equal((await app.state()).tasks.length, 1);
  await deliver(payload('msg_new'));
  const next = await until(async () => (await app.state()).tasks.find(task => task.id !== run), 'Resumed trigger did not execute');
  await app.done(next.id);
  const protectedManagement = await wireRequest(app.base + '/triggers', 'GET', { Host: 'public.example' });
  assert.equal(protectedManagement.status, 403, 'Webhook route must not expose owner management');
});

test('owner registers an event source and creates/pauses/removes a trigger through the UI', { timeout: 60000 }, async t => {
  const provider = await composioFixture(); t.after(provider.close);
  const app = await setup(t, false, { COMPOSIO_API_KEY: 'fixture-composio-key', AYATI_COMPOSIO_BASE_URL: provider.url, COMPOSIO_WEBHOOK_SECRET: secret });
  const connection = await app.request('/connections', { toolkit: 'gmail', auth_config_id: 'ac_fixture', label: 'Event Gmail' });
  await app.request(`/connections/${connection.id}/refresh`, {});
  const browser = await chromium.launch(); t.after(() => browser.close());
  const page = await browser.newPage({ viewport: { width: 390, height: 844 } });
  await page.goto(app.base.replace('/api', '/?agent=local-agent'));
  await page.getByRole('button', { name: 'Automation', exact: true }).click();
  await page.getByRole('tab', { name: 'Triggers', exact: true }).click();
  const panel = page.locator('#triggers-content');
  await panel.getByText('No event triggers for this agent.', { exact: true }).waitFor();
  await panel.getByRole('button', { name: 'Watch a new event', exact: true }).click();
  await panel.getByRole('button', { name: 'Start watching', exact: true }).click();
  await panel.getByText('Event source registered.', { exact: false }).waitFor();
  const sourceRequest = provider.calls.find(c => c.path.endsWith('/upsert')).body;
  assert.equal(sourceRequest.connected_account_id, 'ca_fixture');
  assert.equal(sourceRequest.toolkit_versions.gmail, '20261001_00');
  await panel.getByLabel('Trigger name', { exact: true }).fill('New email review');
  await panel.getByLabel('Task instructions', { exact: true }).fill('Summarize the new message');
  await panel.getByRole('button', { name: 'Create trigger', exact: true }).click();
  await panel.getByText('New email review', { exact: true }).waitFor();
  assert.equal((await app.request('/triggers')).triggers.length, 1);
  await panel.getByRole('button', { name: 'Edit trigger', exact: true }).click();
  await panel.getByLabel('Updated trigger name', { exact: true }).fill('Updated email review');
  await panel.getByRole('button', { name: 'Save trigger changes', exact: true }).click();
  await panel.getByText('Updated email review', { exact: true }).waitFor();
  assert.equal((await app.request('/triggers')).triggers[0].revision, 2);
  await mkdir(join(root, '.test-runs/triggers'), { recursive: true });
  await page.screenshot({ path: join(root, '.test-runs/triggers/mobile.png'), fullPage: true });
  await panel.getByRole('button', { name: 'Pause', exact: true }).click();
  await panel.getByRole('button', { name: 'Resume', exact: true }).waitFor();
  await panel.getByRole('button', { name: 'Remove trigger', exact: true }).click();
  await panel.getByText('No event triggers for this agent.', { exact: true }).waitFor();
});
