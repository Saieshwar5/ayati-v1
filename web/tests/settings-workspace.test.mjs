import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

const w = '11111111-1111-4111-8111-111111111111';
const base = `http://localhost/w/${w}/?view=settings`;
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const saved = { name: 'Northstar', timezone: 'Asia/Kolkata', colour: 'teal', approval_backup_hours: 4, models: [{ provider: 'openai', model: 'gpt-5-mini' }],
  default_model: 'gpt-5-mini', delete_at_ms: null, delete_requested_by: null };
const field = label => [...document.querySelectorAll('label')].find(node => node.textContent.startsWith(label))?.querySelector('input,select,textarea');

test('Workspace rules save once, keep the draft after a failure, and list allowed models', async () => {
  let fail = true; const writes = [];
  const run = async ({ button, change, update }, writes) => {
    assert.match(document.body.textContent, /gpt-5-mini.*default for new agents/);
    await change(document.querySelector('[aria-label="Model identifier"]'), 'glm-5-flash');
    await update(() => button('Add model').click());
    await change(field('Approval backup'), '24');
    await update(() => { const save = document.querySelector('.settings-rules button[type=submit]'); save.click(); save.click(); });
    const sent = writes.filter(write => write.path.endsWith('/settings/workspace'));
    assert.equal(sent.length, 1);
    assert.match(document.body.textContent, /Save failed/);
    assert.ok(document.body.textContent.includes('glm-5-flash'), 'the draft stays');
    fail = false;
    await update(() => document.querySelector('.settings-rules button[type=submit]').click());
    const last = writes.filter(write => write.path.endsWith('/settings/workspace')).at(-1).body;
    assert.equal(last.approval_backup_hours, 24);
    assert.deepEqual(last.models.map(m => m.model), ['gpt-5-mini', 'glm-5-flash']);
    assert.equal(last.default_model, 'gpt-5-mini');
    assert.match(document.body.textContent, /Workspace settings saved/);
  };
  await reactFixture(async context => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { SettingsPage } = await import('../src/react/settings/settings-page.tsx');
    await context.render(SettingsPage, { href: `${base}&section=workspace` });
    await run(context, writes);
  }, { url: base, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response({ user: { id: 'me', email: 'me@test' }, workspaces: [{ id: w, name: 'Northstar', role: 'Admin' }] });
    if (options.body) { writes.push({ path, body: JSON.parse(options.body) }); if (fail) return response({ error: 'Save failed' }, 400); return response({ ...saved, ...JSON.parse(options.body) }); }
    return response(saved);
  } });
});
test('Deleting needs the exact name, shows the date, and can be undone', async () => {
  let state = { ...saved };
  const posts = [];
  await reactFixture(async ({ render, button, click, change }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { SettingsPage } = await import('../src/react/settings/settings-page.tsx');
    await render(SettingsPage, { href: `${base}&section=workspace` });
    await click(button('Delete workspace…'));
    assert.equal(button('Delete in 7 days').disabled, true);
    await change(field('Type the workspace name'), 'Northstar');
    await click(button('Delete in 7 days'));
    assert.deepEqual(posts[0], { path: 'deletion', body: { name: 'Northstar' } });
    assert.match(document.querySelector('.settings-danger').textContent, /Scheduled for .* by Ravi/);
    await click(button('Undo deletion'));
    assert.equal(posts[1].path, 'cancel');
    assert.match(document.body.textContent, /Deletion cancelled/);
    assert.ok(button('Delete workspace…'));
  }, { url: base, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response({ user: { id: 'me', email: 'me@test' }, workspaces: [{ id: w, name: 'Northstar', role: 'Admin' }] });
    if (path.endsWith('/settings/deletion')) { posts.push({ path: 'deletion', body: JSON.parse(options.body) }); state = { ...state, delete_at_ms: Date.now() + 7 * 86400000, delete_requested_by: 'Ravi' }; return response({ delete_at_ms: state.delete_at_ms }); }
    if (path.endsWith('/settings/deletion/cancel')) { posts.push({ path: 'cancel' }); state = { ...state, delete_at_ms: null, delete_requested_by: null }; return response({ saved: true }); }
    return response(state);
  } });
});

test('Preferences save each change to the server and Help sends one report per click', async () => {
  const sent = [];
  let reportFails = true;
  await reactFixture(async ({ render, button, click, change, update }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { SettingsPage } = await import('../src/react/settings/settings-page.tsx');
    await render(SettingsPage, { href: `${base}&section=preferences` });
    await click(document.querySelector('[role=switch]'));
    await change(document.querySelector('select[aria-label="Channels"]'), 'all');
    const prefs = sent.filter(s => s.path.endsWith('/preferences'));
    assert.deepEqual(prefs.at(-1).body, { desktop_alerts: false, channel_level: 'all', approval_alerts: true });
    assert.match(document.body.textContent, /Email alerts.*Coming later/);
    await render(SettingsPage, { href: `${base}&section=help` });
    await change(field('What happened?'), 'Atlas emailed the wrong customer');
    await click(document.querySelector('.settings-check input'));
    await update(() => { const node = button('Send report'); node.click(); node.click(); });
    const reports = () => sent.filter(s => s.path.endsWith('/problem-reports'));
    assert.equal(reports().length, 1);
    assert.equal(reports()[0].body.technical, undefined, 'unticked: no technical details');
    assert.match(document.body.textContent, /Report unavailable/);
    reportFails = false; await click(button('Send report'));
    // The retry reuses the same id, so the server stores one report.
    assert.equal(reports()[1].body.id, reports()[0].body.id);
    assert.match(document.body.textContent, /Report sent/);
  }, { url: base, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response({ user: { id: 'me', email: 'me@test' }, workspaces: [{ id: w, name: 'Northstar', role: 'Member' }] });
    const body = options.body ? JSON.parse(options.body) : undefined;
    if (body) sent.push({ path, body });
    if (path.endsWith('/problem-reports')) return reportFails ? response({ error: 'Report unavailable' }, 503) : response({ id: body.id, saved: true });
    if (path.endsWith('/preferences')) return response(body ?? { desktop_alerts: true, channel_level: 'mentions', approval_alerts: true });
    return response({});
  } });
});

test('Environment lets the owner pick only allowed models and explains a removed one', async () => {
  const writes = [];
  await reactFixture(async ({ render, change, button, click }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { EnvironmentTab } = await import('../src/react/agents/environment-tab.tsx');
    const agent = { id: 'a1', name: 'Atlas', role: 'Orders', model: 'old-model', audience: 'everyone', revision: 3, editable: true,
      settings: { provider: 'openai', instructions: 'x', escalation: '', avatar: 'spark', memory: false, browser: false, history: 'all', paused: false } };
    const directory = { agents: [], role: 'Admin', default_model: 'gpt-5-mini', system_prompt: '', providers: [{ id: 'openai', name: 'OpenAI', configured: true }],
      models: [{ provider: 'openai', model: 'gpt-5-mini' }, { provider: 'openai', model: 'gpt-5' }] };
    await render(EnvironmentTab, { agent, directory, saved: async () => {} });
    assert.match(document.body.textContent, /no longer allowed in this workspace/);
    const picker = field('Model');
    assert.deepEqual([...picker.options].map(o => o.textContent), ['old-model · no longer allowed', 'gpt-5-mini · OpenAI · workspace default', 'gpt-5 · OpenAI']);
    await change(picker, 'openai/gpt-5');
    await click(button('Save model'));
    assert.equal(writes[0].model, 'gpt-5'); assert.equal(writes[0].settings.provider, 'openai');
  }, { url: base, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response({ user: { id: 'me', email: 'me@test' }, workspaces: [{ id: w, name: 'Northstar', role: 'Admin' }] });
    if (options.body) { const body = JSON.parse(options.body); writes.push(body); return response({ ...body, revision: 4 }); }
    return response({});
  } });
});
