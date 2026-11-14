import { registerHooks } from 'node:module';
registerHooks({
  load(url, c, next) {
    return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, c);
  },
});
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const w = '11111111-1111-4111-8111-111111111111',
  url = `http://localhost/w/${w}/?view=agents`;
const response = (value, status = 200) => ({ ok: status < 400, status, json: async () => value });
const agent = { id: 'agent-1', name: 'Study Coach' };
const listed = [
  { name: 'get_marks', title: 'Read exam marks', description: "A student's exam marks", suggested: 'read', supported: true, reason: null, fields: ['student'] },
  { name: 'save_study_plan', title: 'save_study_plan', description: 'Saves a study plan', suggested: 'write', supported: true, reason: null, fields: ['student', 'plan'] },
  { name: 'export_students', title: 'export_students', description: '', suggested: null, supported: false,
    reason: 'Its field “filter” is a nested object or a choice of shapes, which Cuckoo can’t check yet.', fields: [] },
];
const tile = (state = 'connected', held = []) => ({
  id: 'sys-1', name: 'StudyHub', url: 'https://studyhub.example/mcp', has_key: true, revision: 1, state, held,
  last_call_ms: null, created_by: 'Owner',
  tools: [{ name: 'get_marks', kind: 'read', rule: 'allow' }, { name: 'save_study_plan', kind: 'write', rule: 'ask' }],
});
const field = (label) =>
  [...document.querySelectorAll('label')].find((n) => n.textContent.startsWith(label))?.querySelector('input,textarea,select');
async function setup() {
  const context = await import('../src/workspace-context.ts');
  await context.enterWorkspace();
}
const session = () => response({ user: { id: 'owner', email: 'owner@example.test' }, workspaces: [{ id: w, name: 'School', role: 'Admin' }] });

test('connecting a system reviews each tool and saves one exact request without keeping the key on screen', async () => {
  const writes = [];
  let systems = { systems: [], can_connect: true };
  await reactFixture(
    async ({ render, button, click, change, update }) => {
      await setup();
      const { SystemsSection } = await import('../src/react/agents/systems/systems-section.tsx');
      await render(SystemsSection, { agent });
      await click(button('Connect a system'));
      await change(field('Name'), 'StudyHub');
      await change(field('MCP address'), 'https://studyhub.example/mcp');
      await change(field('Key'), 'secret-studyhub-key');
      await click(button('Find its tools'));
      assert.equal(writes.filter((x) => x.path.endsWith('/systems')).length, 0, 'finding tools saves nothing');
      assert.match(document.body.textContent, /Not supported: Its field “filter” is a nested object/);
      const radios = (title) => [...document.querySelectorAll(`[aria-label="${title} rule"] input`)];
      assert.equal(radios('Read exam marks').find((r) => r.checked).value, 'allow', 'reads start on Allow');
      assert.equal(radios('save_study_plan').find((r) => r.checked).value, 'ask', 'changes start on Ask first');
      await update(() => {
        button('Connect StudyHub').click();
        button('Connect StudyHub').click();
      });
      const saves = writes.filter((x) => x.path.endsWith('/systems'));
      assert.equal(saves.length, 1, 'a double click sends one request');
      assert.deepEqual(saves[0].body.tools, [
        { name: 'get_marks', kind: 'read', rule: 'allow' },
        { name: 'save_study_plan', kind: 'write', rule: 'ask' },
      ]);
      assert.equal(saves[0].body.key, 'secret-studyhub-key');
      assert.match(document.body.textContent, /StudyHub is connected/);
      assert.ok(!document.body.innerHTML.includes('secret-studyhub-key'), 'the key is not kept on screen');
    },
    {
      url,
      fetch: async (path, init = {}) => {
        if (path.endsWith('/session')) return session();
        if (init.method === 'POST') {
          const body = JSON.parse(init.body);
          writes.push({ path, body });
          if (path.endsWith('/preview')) return response({ tools: listed });
          if (path.endsWith('/systems')) {
            systems = { systems: [tile()], can_connect: true };
            return response(tile());
          }
        }
        if (path.endsWith('/systems')) return response(systems);
        return response({});
      },
    },
  );
});

test('a changed system shows its held tools; members see tiles but cannot change them', async () => {
  await reactFixture(
    async ({ render }) => {
      await setup();
      const { SystemsSection } = await import('../src/react/agents/systems/systems-section.tsx');
      await render(SystemsSection, { agent });
      assert.match(document.body.textContent, /Changed since it was reviewed/);
      assert.match(document.body.textContent, /1 tool changed since the last review and is held/);
      assert.match(document.body.textContent, /get_marks.*Reads · Allow · held/);
      assert.equal([...document.querySelectorAll('button')].some((b) => /Review|Connect a system|Switch off|Remove/.test(b.textContent)), false);
    },
    {
      url,
      fetch: async (path) => {
        if (path.endsWith('/session')) return session();
        if (path.endsWith('/systems')) return response({ systems: [tile('changed', ['get_marks'])], can_connect: false });
        return response({});
      },
    },
  );
});

test('switching a system off sends its saved revision once', async () => {
  const writes = [];
  await reactFixture(
    async ({ render, button, update }) => {
      await setup();
      const { SystemsSection } = await import('../src/react/agents/systems/systems-section.tsx');
      await render(SystemsSection, { agent });
      await update(() => {
        button('Switch off').click();
        button('Switch off').click();
      });
      assert.equal(writes.length, 1);
      assert.deepEqual(writes[0], { revision: 1, change: 'off' });
      assert.match(document.body.textContent, /StudyHub is off\. Nothing is sent to it\./);
    },
    {
      url,
      fetch: async (path, init = {}) => {
        if (path.endsWith('/session')) return session();
        if (init.method === 'POST') {
          writes.push(JSON.parse(init.body));
          return response({ id: 'sys-1', state: 'off' });
        }
        if (path.endsWith('/systems')) return response({ systems: [tile(writes.length ? 'off' : 'connected')], can_connect: true });
        return response({});
      },
    },
  );
});
