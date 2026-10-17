import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { setup, until, root } from './fixtures/goose_app.mjs';
import { composioMcpFixture } from './fixtures/composio_mcp.mjs';
const { chromium } = createRequire(join(root, 'web/package.json'))('playwright');

test('consumer MCP keeps discovery, account selection and execution behind exact agent grants', { timeout: 120000 }, async t => {
  const remote = await composioMcpFixture(); t.after(remote.close);
  const app = await setup(t, false, { COMPOSIO_API_KEY: 'ck_fixture', AYATI_COMPOSIO_BASE_URL: remote.url, AYATI_MEMORY: '0', AYATI_BROWSER: '0' });
  assert.equal((await app.request('/connections')).configured, true);
  assert.equal((await app.request('/connections/configs')).mode, 'consumer_mcp');
  const accounts = await app.request('/connections/accounts?toolkit=gmail');
  assert.equal(accounts.accounts[0].status, 'ACTIVE'); assert(!JSON.stringify(accounts).includes('DO_NOT_EXPOSE'));
  const linked = await app.request('/connections', { toolkit: 'gmail', auth_config_id: 'consumer_mcp', account_id: 'ca_consumer', label: 'Consumer Gmail' });
  const denied = await fetch(app.base + `/connections/${linked.id}/grants`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ tool: 'COMPOSIO_REMOTE_BASH_TOOL', version: 'guessed', enabled: true }) });
  assert.equal(denied.ok, false);
  const found = await app.request(`/connections/${linked.id}/tools?query=read%20inbox`);
  const grant = { tool: found.tools[0].slug, version: found.tools[0].version, enabled: true };
  assert.match(grant.version, /^mcp_[0-9a-f]{64}$/);
  await app.request(`/connections/${linked.id}/grants`, grant);
  const args = { connection_id: linked.id, tool: grant.tool, arguments: { max_results: 1 } };
  const executions = () => remote.calls.filter(c => c.params?.name === 'COMPOSIO_MULTI_EXECUTE_TOOL');
  async function run(input = args, before = async () => {}) {
    const task = await app.submit('workspace ' + JSON.stringify({ tool: 'connector_execute', arguments: input }));
    const permission = await until(async () => (await app.state()).permissions?.[0], 'Missing MCP operation approval');
    await before();
    if ((await app.state()).permissions.some(p => p.id === permission.id)) await app.request(`/permissions/${permission.id}`, { allow: true });
    return app.done(task.id);
  }
  const outcome = await run(); assert.equal(outcome.task.status, 'completed');
  assert(outcome.state.messages.some(m => m.content.includes('Synthetic consumer inbox')));
  assert.deepEqual(executions()[0].params.arguments.tools, [{ tool_slug: grant.tool, account: 'ca_consumer', arguments: { max_results: 1 } }]);
  await run({ ...args, tool: 'COMPOSIO_REMOTE_WORKBENCH' }); assert.equal(executions().length, 1);
  remote.changed(true); await run(); remote.changed(false); assert.equal(executions().length, 1, 'Schema drift requires renewed consent');
  remote.active(false); await run(); remote.active(true); assert.equal(executions().length, 1, 'Expired account must not dispatch');
  await run(args, () => app.request(`/connections/${linked.id}/grants`, { ...grant, enabled: false })); assert.equal(executions().length, 1);
  await app.request(`/connections/${linked.id}/grants`, grant);
  remote.loseReply(true); const uncertain = await run(); remote.loseReply(false);
  assert.notEqual(uncertain.task.status, 'completed'); assert.equal(executions().length, 2);
  assert.equal((await app.request('/connector-executions')).executions[0].status, 'uncertain');
  const initialized = remote.calls.filter(c => c.method === 'initialize').length;
  await app.request(`/connections/${linked.id}/refresh`, {});
  assert.equal(remote.calls.filter(c => c.method === 'initialize').length, initialized + 1);
  assert.equal(executions().length, 2, 'Transport recovery cannot repeat the uncertain operation');
  await app.stop(); await app.start(); assert.equal(executions().length, 2);
  await app.request(`/connections/${linked.id}/revoke`, {}); await run(); assert.equal(executions().length, 2);
  const added = await app.request('/connections', { toolkit: 'gmail', auth_config_id: 'consumer_mcp', label: 'Another Gmail' });
  assert.equal(added.redirect_url, 'https://connect.composio.dev/fixture'); assert(!JSON.stringify(added).includes('DO_NOT_EXPOSE'));
});

test('consumer account selection works in Settings without project auth configurations', { timeout: 60000 }, async t => {
  const remote = await composioMcpFixture(); t.after(remote.close);
  const app = await setup(t, false, { COMPOSIO_API_KEY: 'ck_fixture', AYATI_COMPOSIO_BASE_URL: remote.url });
  const browser = await chromium.launch(); t.after(() => browser.close());
  const page = await browser.newPage({ viewport: { width: 390, height: 844 } });
  await page.goto(app.base.replace('/api', '/?view=settings'));
  await page.getByRole('button', { name: 'Connect an account', exact: true }).click();
  await page.getByLabel('Composio account', { exact: true }).waitFor();
  await page.getByLabel('Account label', { exact: true }).fill('My MCP Gmail');
  await page.getByRole('button', { name: 'Use selected account or create sign-in link', exact: true }).click();
  await page.getByText('gmail · ACTIVE', { exact: true }).waitFor();
  assert(!(await page.content()).includes('DO_NOT_EXPOSE'));
});
