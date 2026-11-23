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

test('Models and the approval backup save as they change, and a refused change keeps what was typed', async () => {
  let fail = true; const writes = [];
  const sent = () => writes.filter(write => write.path.endsWith('/settings/workspace'));
  await reactFixture(async ({ render, button, change, update, click }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { SettingsPage } = await import('../src/react/settings/settings-page.tsx');
    await render(SettingsPage, { href: `${base}&section=company-rules` });
    assert.equal(document.querySelector('.settings-detail h2').textContent, 'Agent rules');
    assert.match(document.querySelector('.settings-models').textContent, /gpt-5-mini.*Default/);
    const model = () => document.querySelector('[aria-label="Model identifier"]');
    await change(model(), 'glm-5-flash');
    await update(() => { const add = button('Add model'); add.click(); add.click(); });
    assert.equal(sent().length, 1, 'one request for a double click');
    assert.match(document.body.textContent, /Save failed/);
    assert.equal(model().value, 'glm-5-flash', 'the typed model stays');
    fail = false;
    await click(button('Add model'));
    assert.deepEqual(sent().at(-1).body.models.map(m => m.model), ['gpt-5-mini', 'glm-5-flash']);
    assert.equal(sent().at(-1).body.default_model, 'gpt-5-mini');
    assert.equal(model().value, '');
    await change(document.querySelector('[aria-label="Approval backup"]'), '24');
    assert.equal(sent().at(-1).body.approval_backup_hours, 24);
    // Every write carries the whole settings, so nothing else is reset.
    assert.deepEqual([sent().at(-1).body.timezone, sent().at(-1).body.colour], ['Asia/Kolkata', 'teal']);
    await click([...document.querySelectorAll('input[name=default-model]')].at(-1));
    assert.equal(sent().at(-1).body.default_model, 'glm-5-flash');
  }, { url: base, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response({ user: { id: 'me', email: 'me@test' }, workspaces: [{ id: w, name: 'Northstar', role: 'Admin' }] });
    if (path.endsWith('/guard/rules')) return response({ rules: [], actions: [], can_manage: true });
    if (options.body) { writes.push({ path, body: JSON.parse(options.body) }); if (fail) return response({ error: 'Save failed' }, 400); return response({ ...saved, ...JSON.parse(options.body) }); }
    return response(saved);
  } });
});

test('General saves the name and time zone from one bar, and shows a stored zone the browser lists under another name', async () => {
  const writes = [];
  let failZone = true;
  await reactFixture(async ({ render, button, change, click, submit }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { SettingsPage } = await import('../src/react/settings/settings-page.tsx');
    await render(SettingsPage, { href: `${base}&section=workspace` });
    assert.equal(document.querySelector('.settings-detail h2').textContent, 'General');
    const zone = field('Time zone');
    assert.equal(zone.value, 'Asia/Kolkata', 'the stored zone is selected even where the browser calls it Asia/Calcutta');
    assert.match(zone.selectedOptions[0].textContent, /^\(GMT\+5:30\) Kolkata/);
    assert.equal(document.querySelector('.settings-save-bar'), null, 'no bar until something changes');
    assert.doesNotMatch(document.body.textContent, /Colour|Export data|Coming later/);
    await change(field('Workspace name'), 'Northstar Home');
    await change(zone, 'UTC');
    const form = document.querySelector('form.settings-general');
    await submit(form);
    // The name saved; the time zone was refused and stays as a draft.
    assert.deepEqual(writes.map(write => write.path.split('/').at(-1)), ['settings', 'workspace']);
    assert.deepEqual(writes[0].body, { name: 'Northstar Home', previous_name: 'Northstar' });
    assert.match(document.body.textContent, /Zone refused/);
    assert.equal(zone.value, 'UTC');
    assert.match(document.querySelector('.settings-heading p').textContent, /Northstar Home/);
    failZone = false;
    await submit(form);
    assert.equal(writes.length, 3, 'only the time zone is sent again');
    assert.deepEqual([writes[2].body.timezone, writes[2].body.colour, writes[2].body.approval_backup_hours], ['UTC', 'teal', 4]);
    assert.equal(document.querySelector('.settings-save-bar'), null);
    await change(field('Workspace name'), 'Something else');
    await click(button('Discard'));
    assert.equal(field('Workspace name').value, 'Northstar Home');
  }, { url: base, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response({ user: { id: 'me', email: 'me@test' }, workspaces: [{ id: w, name: 'Northstar', role: 'Admin' }] });
    if (path.endsWith('/chat-directory')) return response({ role: 'Admin', users: [{ id: 'me', name: 'Me' }], agents: [] });
    if (options.body) {
      const body = JSON.parse(options.body); writes.push({ path, body });
      if (path.endsWith('/settings')) return response({ name: body.name });
      return failZone ? response({ error: 'Zone refused' }, 400) : response({ ...saved, ...body });
    }
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

test('Preferences save each change to the server, with no placeholder rows', async () => {
  const sent = [];
  await reactFixture(async ({ render, click, change }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { SettingsPage } = await import('../src/react/settings/settings-page.tsx');
    await render(SettingsPage, { href: `${base}&section=preferences` });
    await click(document.querySelector('[role=switch]'));
    await change(document.querySelector('select[aria-label="Channels"]'), 'all');
    const prefs = sent.filter(s => s.path.endsWith('/preferences'));
    assert.deepEqual(prefs.at(-1).body, { desktop_alerts: false, channel_level: 'all', approval_alerts: true });
    const page = document.querySelector('.settings-detail');
    assert.doesNotMatch(page.textContent, /Coming later|Email alerts/);
    assert.deepEqual([...page.querySelectorAll('.settings-row strong')].map(node => node.textContent), ['Desktop alerts', 'Channels', 'Approvals for agents I manage']);
    assert.equal(page.querySelector('.settings-notifications a').getAttribute('href'), `/w/${w}/?view=settings&section=profile`);
  }, { url: base, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response({ user: { id: 'me', email: 'me@test' }, workspaces: [{ id: w, name: 'Northstar', role: 'Member' }] });
    const body = options.body ? JSON.parse(options.body) : undefined;
    if (body) sent.push({ path, body });
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
