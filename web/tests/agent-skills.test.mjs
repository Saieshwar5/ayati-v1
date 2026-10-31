import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=agents`;
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const files = [{ path: 'SKILL.md', text: '---\nname: quote-builder\ndescription: Use when asked for a quote\n---\n1. Check prices.\n' },
  { path: 'references/prices.csv', text: 'item,price\nchair,4500\n' }];

test('a downloaded skill unzips to the same files, and unsafe archives are refused', async () => {
  const { readZip, zipSkill } = await import('../src/react/agents/skills/model.ts');
  const { zipSync, strToU8 } = await import('fflate');
  assert.deepEqual(readZip(zipSkill('quote-builder', files)), files, 'the top folder is removed on upload');
  assert.throws(() => readZip(zipSync({ 'logo.png': new Uint8Array([0xff, 0xd8, 0xff, 0x00, 0x80]) })), /isn't a text file/);
  // Six megabytes of zeros compress to a few kilobytes; the size it expands to counts.
  const bomb = zipSync({ 'SKILL.md': strToU8('---\nname: a\ndescription: d\n---\n'), 'big.txt': new Uint8Array(6 * 1024 * 1024) });
  assert.ok(bomb.length < 100 * 1024);
  assert.throws(() => readZip(bomb), /at most 5 MB/);
  const many = Object.fromEntries(Array.from({ length: 101 }, (_, n) => [`r/${n}.md`, strToU8('x')]));
  assert.throws(() => readZip(zipSync(many)), /at most 100 files/);
});

test("an agent's skills list, add from the library or by upload, and remove", async () => {
  const writes = [];
  let assigned = [];
  const shared = { id: 's1', name: 'quote-builder', description: 'Use when asked for a quote', shared: true, owner_id: 'admin', owner_name: 'Ravi', version: 3, license: null, has_scripts: false, updated_ms: 1, files: 2, agents: [] };
  const theirs = { ...shared, id: 's2', name: 'their-steps', shared: false, owner_id: 'someone-else' };
  await reactFixture(async ({ render, button, click, change, update }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { AgentSkills } = await import('../src/react/agents/skills/agent-skills.tsx');
    const agent = { id: 'a1', name: 'Atlas', role: 'Orders', model: 'm', audience: 'everyone', revision: 1, owner_id: 'me', editable: true, settings: {} };
    await render(AgentSkills, { agent });
    assert.match(document.body.textContent, /No skills yet/);
    await click(button('＋ Add skill'));
    const drawer = () => document.querySelector('dialog');
    assert.match(drawer().textContent, /Workspace skillsquote-builder/);
    // Someone else's personal skill can't go on this agent.
    const rows = [...drawer().querySelectorAll('.skill-list li')];
    assert.equal(rows.find(r => r.textContent.includes('their-steps')).querySelector('button').disabled, true);
    await click(rows.find(r => r.textContent.includes('quote-builder')).querySelector('button'));
    assert.deepEqual(writes.at(-1).body, { skill: 's1', on: true });
    // Upload: paste, check, review every file, save once, then add it.
    await click(button('Upload a new skill'));
    await change(drawer().querySelector('textarea'), '---\nname: delivery-update\ndescription: Use when a delivery date changes\n---\n1. Tell the customer.');
    await click(button('Check pasted skill'));
    assert.match(drawer().textContent, /delivery-update.*Use it when: Use when a delivery date changes.*No license/s);
    await update(() => { const save = button('Save skill'); save.click(); save.click(); });
    assert.equal(writes.filter(w => w.path.endsWith('/skills') && !w.path.includes('/agent-profiles/')).length, 1);
    assert.deepEqual(writes.at(-1).body, { skill: 's3', on: true });
    await click(drawer().querySelector('[aria-label="Close"]'));
    assert.match(document.querySelector('.agent-skills').textContent, /quote-builderWorkspace skill · version 3 · used 2 times this week/);
    await click(button('Actions for quote-builder'));
    await click(button('Remove from this agent'));
    assert.deepEqual(writes.at(-1).body, { skill: 's1', on: false });
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response({ user: { id: 'me', email: 'me@test' }, workspaces: [{ id: w, name: 'Store', role: 'Member' }] });
    const body = options.body ? JSON.parse(options.body) : undefined;
    if (body) writes.push({ path, body });
    if (path.endsWith('/agent-profiles/a1/skills')) {
      if (body) assigned = body.on ? [...assigned.filter(s => s.id !== body.skill), { ...(body.skill === 's1' ? shared : { ...shared, id: 's3', name: 'delivery-update', shared: false }), used_this_week: 2 }] : assigned.filter(s => s.id !== body.skill);
      return response({ skills: assigned, editable: true });
    }
    if (path.endsWith('/skills/check')) return response({ name: 'delivery-update', description: 'Use when a delivery date changes', license: null, has_scripts: false, hash: 'h', bytes: 10 });
    if (path.endsWith('/skills') && body) return response({ ...shared, id: 's3', name: 'delivery-update', shared: false, owner_id: 'me', files: body.files, versions: [], hash: 'h', shown_version: 1 });
    if (path.endsWith('/skills')) return response({ workspace: [shared], mine: [], others: [theirs], admin: true });
    return response({});
  } });
});
