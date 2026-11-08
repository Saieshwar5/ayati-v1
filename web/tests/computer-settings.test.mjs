import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

// Settings → Computers (designs/agent-computers 5): admins limit every agent's computer.
const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=settings&section=computers`;
const response = data => ({ ok: true, json: async () => data });
const open = { allowed: true, sizes: ['small', 'standard', 'large'], internet: 'any', websites: [], programs: 'allow', browser: 'allow', submit: 'allow' };
const radio = (group, label) => [...document.querySelector(`[aria-label="${group}"]`).querySelectorAll('label')].find(node => node.textContent === label).querySelector('input');
const check = label => [...document.querySelectorAll('label')].find(node => node.textContent === label).querySelector('input');

test('an admin limits sizes, internet and rules, saving them together', async () => {
  const saves = [];
  await reactFixture(async ({ render, button, click, change }) => {
    const { ComputersSettings } = await import('../src/react/settings/computers-settings.tsx');
    await render(ComputersSettings);
    assert.equal(button('Save computer limits').disabled, true);
    await click(check('Large'));
    await click(radio('Internet', 'Only these websites'));
    const field = [...document.querySelectorAll('label')].find(node => node.textContent.startsWith('Add a website')).querySelector('input');
    await change(field, '*.zoho.in');
    await click(button('Add'));
    await click(radio('Press buttons and send forms on websites', 'Ask first'));
    await click(button('Save computer limits'));
    assert.equal(saves.length, 1);
    assert.deepEqual(saves[0], { ...open, sizes: ['small', 'standard'], internet: 'listed', websites: ['*.zoho.in'], submit: 'ask' });
    assert.match(document.body.textContent, /Computer limits saved/);
    // Switched off, nothing else is shown and every computer stops at its next use.
    await click(check('Allow computers'));
    assert.equal(document.querySelector('.settings-computer-sizes'), null);
  }, { url, fetch: async (path, options = {}) => {
    if (!options.body) return response({ policy: open, can_change: true, configured: true });
    const body = JSON.parse(options.body); saves.push(body);
    return response({ policy: body, can_change: true, configured: true });
  } });
});

test('members read the limits without changing them, and an unset Cuckoo says so', async () => {
  await reactFixture(async ({ render }) => {
    const { ComputersSettings } = await import('../src/react/settings/computers-settings.tsx');
    await render(ComputersSettings);
    assert.equal(document.querySelector('fieldset.ui-fieldset').disabled, true);
    assert.equal([...document.querySelectorAll('button')].some(node => node.textContent === 'Save computer limits'), false);
    assert.match(document.body.textContent, /Only admins change these/);
    assert.match(document.body.textContent, /Computers aren’t set up on this Cuckoo yet/);
  }, { url, fetch: async () => response({ policy: open, can_change: false, configured: false }) });
});

test('admins set how many computers may be awake and see each agent’s month', async () => {
  const saves = [];
  const computers = [{ agent: 'atlas', name: 'Atlas', size: 'standard', state: 'awake', in_use: true, waiting: 2, seconds: 7500, charge_usd: 0.35 }];
  await reactFixture(async ({ render, button, click, change }) => {
    const { ComputersSettings } = await import('../src/react/settings/computers-settings.tsx');
    await render(ComputersSettings);
    const table = [...document.querySelectorAll('table')].find(node => node.querySelector('caption').textContent === 'Agents’ computers this month');
    assert.match(table.textContent, /AtlasStandardWorking, in use · 2 waiting2 h 5 min\$0\.35/);
    const awake = [...document.querySelectorAll('label')].find(node => node.textContent.startsWith('Computers awake at once')).querySelector('input');
    await change(awake, '3');
    assert.match(document.body.textContent, /When 3 are working, other agents wait their turn/);
    await click(button('Save computer limits'));
    assert.equal(saves[0].max_awake, 3);
  }, { url, fetch: async (path, options = {}) => {
    if (!options.body) return response({ policy: { ...open, max_awake: 0 }, can_change: true, configured: true, computers });
    const body = JSON.parse(options.body); saves.push(body);
    return response({ policy: body, can_change: true, configured: true, computers });
  } });
});

test('operators see every computer by provider and pause them all with a reason', async () => {
  const writes = [];
  const data = { configured: true, paused: false,
    sizes: [{ size: 'small', cpu: 1, memory_gb: 2, usd_per_hour: 0.09, routes: [{ provider: 'e2b', image: 'cuckoo-computer-small', usd_per_hour: 0.06 }] }],
    providers: [{ provider: 'e2b', computers: 3, awake: 1, problems: 1, seconds: 36000, charge: 2.5, cost: 1.6 }],
    computers: [{ workspace: 'w1', workspace_name: 'Mehta Furnishings', agent: 'a1', agent_name: 'Atlas', size: 'small', provider: 'e2b', provider_id: 'sbx-1',
      state: 'awake', awake_since: 1, last_used: 1762600000, problem: null, in_use: true, seconds: 3600, charge: 0.09, cost: 0.06 }] };
  await reactFixture(async ({ render, button, click, change }) => {
    const { ComputersScreen } = await import('../src/control/computers.tsx');
    await render(ComputersScreen);
    const text = document.body.textContent;
    assert.match(text, /Awake now1of 1 computers/);
    assert.match(text, /\$2\.50/);
    assert.match(text, /margin \$0\.90/);
    assert.match(text, /e2b: cuckoo-computer-small · \$0\.06/);
    assert.match(text, /AtlasMehta FurnishingsWorking/);
    await click(button('Pause all computers'));
    const reason = document.querySelector('dialog textarea');
    await change(reason, 'Provider incident');
    await click([...document.querySelectorAll('dialog button')].find(node => node.textContent === 'Pause all computers'));
    assert.deepEqual(writes, [{ key: 'computers.paused', value: true, reason: 'Provider incident' }]);
    assert.match(document.body.textContent, /Agents’ computers are paused/);
  }, { url: 'http://localhost/control.html?section=computers', fetch: async (path, options = {}) => {
    if (options.body) { writes.push(JSON.parse(options.body)); return response({}); }
    return response(writes.length ? { ...data, paused: true } : data);
  } });
});
