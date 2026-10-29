import { test } from 'node:test';
import assert from 'node:assert/strict';
import { JSDOM } from 'jsdom';

test('agent drawer retains edits after revision conflicts and sends only explicit saves', async () => {
  const keys = ['document', 'window', 'location', 'fetch', 'localStorage', 'setInterval'];
  const previous = Object.fromEntries(keys.map(k => [k, globalThis[k]]));
  const workspace = '11111111-1111-4111-8111-111111111111';
  const dom = new JSDOM('<div id="app"></div>', { url: `http://localhost/w/${workspace}/?view=agents` });
  const writes = []; const profile = { id: 'agent-one', name: 'Operations', role: 'Help with stock', model: 'test-model', state: 'ready', editable: true, revision: 7, memory: false, browser: false, browser_available: false };
  try {
    globalThis.document = dom.window.document; globalThis.window = dom.window; globalThis.location = dom.window.location;
    globalThis.localStorage = { setItem() {}, getItem() { return null; } }; globalThis.setInterval = () => 0; dom.window.setInterval = () => 0;
    dom.window.HTMLDialogElement.prototype.showModal = function () { this.open = true; };
    dom.window.HTMLDialogElement.prototype.close = function () { this.open = false; this.dispatchEvent(new dom.window.Event('close')); };
    globalThis.fetch = async (path, options = {}) => {
      if (options.method === 'POST') {
        writes.push({ path, body: JSON.parse(options.body) });
        assert.equal(options.headers.get('X-Ayati-Request'), 'identity');
        return { ok: false, json: async () => ({ error: 'Profile changed. Reload before saving.' }) };
      }
      const data = path.endsWith('/session') ? { user: { id: 'owner' }, workspaces: [{ id: workspace, name: 'Test', role: 'Owner' }] }
        : path.endsWith('/context') ? {}
        : { agents: [{ profile }], creation_available: true, default_model: 'test-model', memory_available: false, browser_available: false };
      return { ok: true, json: async () => data };
    };
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { installAgentManager } = await import('../src/agent-manager.ts'); await installAgentManager();
    [...document.querySelectorAll('button')].find(b => b.textContent === 'Manage').click();
    const form = document.querySelector('dialog form');
    assert.equal(writes.length, 0);
    form.elements.name.value = 'Stock assistant';
    await form.onsubmit(new dom.window.Event('submit'));
    assert.deepEqual(writes, [{ path: `/w/${workspace}/api/agents/agent-one/profile`, body: { name: 'Stock assistant', role: 'Help with stock', model: 'test-model', memory: false, browser: false, expected_revision: 7 } }]);
    assert.equal(form.elements.name.value, 'Stock assistant');
    assert.match(document.querySelector('dialog [role=status]').textContent, /Profile changed/);
    assert.equal(form.elements.memory.disabled, true); assert.equal(form.elements.browser.disabled, true);
    assert.equal(document.querySelector('dialog [type=submit]').disabled, false);
    form.elements.model.dispatchEvent(new dom.window.Event('invalid', { cancelable: true }));
    const active = document.querySelector('dialog [role=tab][aria-selected=true]');
    assert.equal(active.textContent, 'Advanced');
    assert.equal(writes.length, 1);
  } finally { Object.assign(globalThis, previous); dom.window.close(); }
});
