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
