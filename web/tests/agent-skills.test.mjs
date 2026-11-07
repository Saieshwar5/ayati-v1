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
const steps = Array.from({ length: 8 }, (_, n) => `${n + 1}. Step ${n + 1}.`).join('\n');
const measurement = [
  { path: 'SKILL.md', text: `---\nname: site-measurement\ndescription: A customer sends room measurements\nlicense: MIT\n---\n${steps}\n` },
  { path: 'reference/fabric-widths.md', text: '# Widths\n' },
  { path: 'scripts/cutting_list.py', text: 'print("cut")\n' },
];
const pasted = '---\nname: delivery-update\ndescription: Use when a delivery date changes\n---\n1. Tell the customer.';

const base = { description: '', shared: true, owner_id: 'ravi', owner_name: 'Ravi Kumar', version: 1, license: null, has_scripts: false, updated_ms: 1, files: 1, agents: [] };
const quote = { ...base, id: 's1', name: 'quote-builder', description: 'Builds a quote from the price list.', version: 3 };
const refund = { ...base, id: 's5', name: 'refund-check', description: 'Checks refund rules before promising one.', owner_name: 'Kavya Nair', owner_id: 'kavya', version: 2 };
const fabric = { ...base, id: 's4', name: 'fabric-care', description: 'Care steps for the 2026 fabrics.', shared: false, owner_id: 'me', owner_name: 'Priya Shah' };
const gst = { ...base, id: 's2', name: 'gst-invoice-check', description: 'Checks GST before an invoice is sent.', shared: false, owner_id: 'kavya', owner_name: 'Kavya Nair' };

/** A fake identity server: the library, checks, uploads and one agent's skills. */
function server({ admin = true } = {}) {
  const state = { writes: [], assigned: [], created: [], failCreate: 0, failAssign: 0, holdCheck: undefined };
  const all = () => [quote, refund, fabric, gst, ...state.created];
  state.creates = () => state.writes.filter(write => /\/workspaces\/[^/]+\/skills$/.test(write.path));
  state.assigns = () => state.writes.filter(write => write.path.includes('/agent-profiles/'));
  state.fetch = async (path, options = {}) => {
    if (path.endsWith('/session')) return response({ user: { id: 'me', email: 'me@test' }, workspaces: [{ id: w, name: 'Store', role: admin ? 'Admin' : 'Member' }] });
    const body = options.body ? JSON.parse(options.body) : undefined;
    if (body) state.writes.push({ path, body });
    if (path.endsWith('/agent-profiles/a1/skills')) {
      if (body && state.failAssign-- > 0) return response({ error: 'An agent can have at most 20 skills.' }, 400);
      if (body) state.assigned = body.on ? [...state.assigned.filter(s => s.id !== body.skill), { ...all().find(s => s.id === body.skill), used_this_week: 2 }]
        : state.assigned.filter(s => s.id !== body.skill);
      return response({ skills: state.assigned, editable: true });
    }
    if (path.endsWith('/skills/check')) {
      await state.holdCheck;
      const text = body.files.find(file => file.path === 'SKILL.md').text;
      return response({ name: text.match(/name: (.+)/)[1], description: text.match(/description: (.+)/)[1], license: text.match(/license: (.+)/)?.[1] ?? null,
        has_scripts: body.files.some(file => file.path.startsWith('scripts/')), hash: 'h', bytes: 10 });
    }
    if (path.endsWith('/skills') && body) {
      if (state.failCreate-- > 0) return response({ error: 'Skills are unavailable right now.' }, 503);
      const text = body.files.find(file => file.path === 'SKILL.md').text;
      const skill = { ...base, id: `s${9 + state.created.length}`, name: text.match(/name: (.+)/)[1], shared: body.shared, owner_id: 'me', owner_name: 'Priya Shah' };
      if (!state.created.some(s => s.name === skill.name)) state.created.push(skill);
      return response({ ...skill, files: body.files, versions: [], hash: 'h', shown_version: 1 });
    }
    if (path.endsWith('/skills')) return response({ workspace: [quote, refund, ...state.created.filter(s => s.shared)], mine: [fabric, ...state.created.filter(s => !s.shared)], others: admin ? [gst] : [], admin });
    return response({});
  };
  return state;
}

const dialog = () => document.querySelector('dialog');
const title = () => dialog().querySelector('.ui-dialog-header h2').textContent;
const row = name => [...dialog().querySelectorAll('.skill-row')].find(node => node.querySelector('strong').textContent === name);
const names = () => [...dialog().querySelectorAll('.skill-row strong')].map(node => node.textContent);
const focused = () => document.activeElement?.getAttribute('aria-label') || document.activeElement?.textContent;
const until = async (update, ready) => { for (let n = 0; n < 20 && !ready(); n++) await update(); assert.ok(ready(), 'the panel settled'); };

/** Render the agent's Skills section and open Add skills. */
async function openPanel({ render, click, button }, owner = 'me') {
  const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
  const { AgentSkills } = await import('../src/react/agents/skills/agent-skills.tsx');
  await render(AgentSkills, { agent: { id: 'a1', name: 'Atlas', role: 'Orders', model: 'm', audience: 'everyone', revision: 1, owner_id: owner, editable: true, settings: {} } });
  await click(button('＋ Add skills'));
}

async function pasteSkill({ button, click, change, update }, text = pasted) {
  await click(button('or paste a SKILL.md'));
  await change(dialog().querySelector('textarea'), text);
  await click(button('Check SKILL.md'));
  await until(update, () => dialog().querySelector('.skill-source'));
}

/** The model reads the workspace id from the page address when first loaded, so it
 * loads inside the fixture's page even for tests that render nothing. */
const withModel = run => reactFixture(async () => run(await import('../src/react/agents/skills/model.ts')), { url });

test('a downloaded skill unzips to the same files, and unsafe archives are refused', () => withModel(async ({ readZip, zipSkill }) => {
  const { zipSync, strToU8 } = await import('fflate');
  assert.deepEqual(readZip(zipSkill('quote-builder', files)), files, 'the top folder is removed on upload');
  assert.throws(() => readZip(zipSync({ 'logo.png': new Uint8Array([0xff, 0xd8, 0xff, 0x00, 0x80]) })), /isn't a text file/);
  // Six megabytes of zeros compress to a few kilobytes; the size it expands to counts.
  const bomb = zipSync({ 'SKILL.md': strToU8('---\nname: a\ndescription: d\n---\n'), 'big.txt': new Uint8Array(6 * 1024 * 1024) });
  assert.ok(bomb.length < 100 * 1024);
  assert.throws(() => readZip(bomb), /at most 5 MB/);
  const many = Object.fromEntries(Array.from({ length: 101 }, (_, n) => [`r/${n}.md`, strToU8('x')]));
  assert.throws(() => readZip(zipSync(many)), /at most 100 files/);
}));

test('a dropped folder is walked with the same limits; a dropped zip or SKILL.md is read', () => withModel(async ({ readDropped, zipSkill }) => {
  const file = (path, text) => ({ isDirectory: false, name: path.split('/').pop(), fullPath: `/${path}`, file: done => done(new File([text], path.split('/').pop())) });
  const folder = (path, children) => ({ isDirectory: true, name: path.split('/').pop(), fullPath: `/${path}`,
    createReader: () => { let read = false; return { readEntries: done => { done(read ? [] : children); read = true; } }; } });
  const dropped = folder('site-measurement', [file('site-measurement/SKILL.md', measurement[0].text), file('site-measurement/.DS_Store', 'x'),
    folder('site-measurement/scripts', [file('site-measurement/scripts/cutting_list.py', measurement[2].text)])]);
  assert.deepEqual(await readDropped([dropped], []), { source: 'site-measurement', files: [measurement[0], measurement[2]] });
  const zip = new File([zipSkill('quote-builder', files)], 'quote-builder.zip');
  assert.deepEqual(await readDropped([file('quote-builder.zip', '')], [zip]), { source: 'quote-builder.zip', files });
  assert.deepEqual((await readDropped([null], [new File([files[0].text], 'SKILL.md')])).files, [files[0]]);
  await assert.rejects(readDropped([null, null], [zip, zip]), /Drop one \.zip, one skill folder or a SKILL\.md/);
  const many = folder('big', Array.from({ length: 101 }, (_, n) => file(`big/${n}.md`, 'x')));
  await assert.rejects(readDropped([many], []), /at most 100 files/);
}));

test('search filters by name, description and owner across the short groups', async () => {
  const fake = server();
  await reactFixture(async tools => {
    await openPanel(tools);
    assert.equal(title(), 'Add skills');
    assert.equal(focused(), 'Search skills', 'the search is focused when the panel opens');
    assert.match(dialog().textContent, /How-to steps Atlas loads only when a request needs them/);
    assert.deepEqual([...dialog().querySelectorAll('.skill-group')].map(node => node.textContent), ['In this workspace · 2', 'Yours · 1', 'Other people’s · 1']);
    const search = dialog().querySelector('input[type=search]');
    await tools.change(search, 'QUOTE');
    assert.deepEqual(names(), ['quote-builder']);
    assert.deepEqual([...dialog().querySelectorAll('.skill-group')].map(node => node.textContent), ['In this workspace · 1'], 'empty groups hide while searching');
    await tools.change(search, 'fabrics');
    assert.deepEqual(names(), ['fabric-care']);
    await tools.change(search, 'kavya');
    assert.deepEqual(names(), ['refund-check', 'gst-invoice-check']);
    await tools.change(search, 'nothing like this');
    assert.deepEqual(names(), []);
    assert.match(dialog().textContent, /No skills match “nothing like this”/);
    assert.equal(fake.writes.length, 0, 'searching never writes');
  }, { url, fetch: fake.fetch });
});

test('Add attaches once, shows Added and keeps focus; unusable skills say why in text', async () => {
  const fake = server();
  await reactFixture(async tools => {
    const { button, click, update } = tools;
    await openPanel(tools);
    assert.match(dialog().textContent, /0 added to Atlas/);
    await update(() => { const add = button('Add quote-builder'); add.focus(); add.click(); add.click(); });
    assert.equal(fake.assigns().length, 1, 'a double click sends one request');
    assert.deepEqual(fake.assigns()[0].body, { skill: 's1', on: true });
    assert.match(row('quote-builder').textContent, /Added$/);
    assert.equal(focused(), 'quote-builder added', 'focus stays on the row');
    assert.match(dialog().textContent, /1 added to Atlas/);
    await click(button('quote-builder added'));
    assert.equal(fake.assigns().length, 1, 'Added does nothing');
    // Another person's personal skill stays visible, greyed, with the reason as text.
    const theirs = row('gst-invoice-check');
    assert.equal(theirs.classList.contains('unusable'), true);
    assert.equal(theirs.querySelectorAll('button').length, 0);
    assert.match(theirs.textContent, /Kavya Nair · personalOnly for Kavya’s agents$/);
    assert.match(row('fabric-care').textContent, /Only you · version 1Add$/);
    // A failed add keeps the row and says why.
    fake.failAssign = 1;
    await click(button('Add refund-check'));
    assert.match(dialog().querySelector('.ui-dialog-footer').textContent, /at most 20 skills/);
    assert.match(row('refund-check').textContent, /Add$/);
  }, { url, fetch: fake.fetch });
});

test("on someone else's agent, an admin's own skills explain they only fit their own agents", async () => {
  const fake = server();
  await reactFixture(async tools => {
    await openPanel(tools, 'kavya');
    assert.match(row('fabric-care').textContent, /Only for your agents$/);
    assert.equal(row('gst-invoice-check').querySelector('button').textContent, 'Add', "Kavya's own skill fits Kavya's agent");
    await tools.click(tools.button('Upload a skill'));
    await pasteSkill(tools);
    assert.equal(dialog().querySelector('.skill-share input').checked, true, 'sharing is on so the agent can use it');
    await tools.click(dialog().querySelector('.skill-share input'));
    assert.match(dialog().textContent, /only works on your own agents, so Atlas can’t use it/);
  }, { url, fetch: fake.fetch });
});

test('upload: choose a .zip by keyboard, review what was read, Add skill, back to the list', async () => {
  const fake = server();
  await reactFixture(async tools => {
    const { dom, button, click, update } = tools;
    const { zipSkill } = await import('../src/react/agents/skills/model.ts');
    await openPanel(tools);
    await click(button('Upload a skill'));
    assert.equal(title(), 'Upload a skill');
    assert.ok(button('Back'), 'a back arrow in the header');
    // The file control is a real button, focused on arrival; it opens the chooser.
    const choose = button('Choose .zip');
    assert.equal(choose.tagName, 'BUTTON');
    assert.equal(choose.tabIndex, 0);
    assert.equal(focused(), 'Choose .zip');
    assert.equal(button('Add skill').disabled, true, 'nothing to add yet');
    const opened = [];
    const { click: original } = dom.window.HTMLInputElement.prototype;
    dom.window.HTMLInputElement.prototype.click = function () { if (this.type === 'file') opened.push(this.accept); else original.call(this); };
    await click(choose);
    assert.deepEqual(opened, ['.zip,application/zip']);
    const input = dialog().querySelector('input[type=file][accept]');
    Object.defineProperty(input, 'files', { configurable: true, value: [new dom.window.File([zipSkill('site-measurement', measurement)], 'site-measurement.zip')] });
    await update(() => input.dispatchEvent(new dom.window.Event('change', { bubbles: true })));
    await until(update, () => dialog().querySelector('.skill-source'));
    assert.match(dialog().querySelector('.skill-source').textContent, /site-measurement\.zip3 files · \d+ B · checkedChoose another/);
    assert.match(dialog().querySelector('.skill-facts').textContent, /Namesite-measurementLicenseMITUse it whenA customer sends room measurements/);
    // One file list: SKILL.md first and open, the others closed, a script tagged.
    const toggles = [...dialog().querySelectorAll('.skill-file-toggle')];
    assert.deepEqual(toggles.map(node => [node.querySelector('.skill-file-path').textContent, node.getAttribute('aria-expanded')]),
      [['SKILL.md', 'true'], ['reference/fabric-widths.md', 'false'], ['scripts/cutting_list.py', 'false']]);
    assert.match(toggles[2].textContent, /Can’t run yet/);
    assert.equal(dialog().querySelectorAll('.skill-file-preview').length, 1);
    assert.doesNotMatch(dialog().querySelector('.skill-file-preview pre').textContent, /Step 8/);
    await click(button('Show all 13 lines'));
    assert.match(dialog().querySelector('.skill-file-preview pre').textContent, /8\. Step 8\./);
    await click(toggles[1]);
    assert.equal(dialog().querySelectorAll('.skill-file-preview').length, 2);
    assert.match(dialog().textContent, /This skill has a script\. Scripts can’t run yet, so Atlas follows the written steps only\./);
    const share = dialog().querySelector('.skill-share input');
    assert.equal(share.checked, false, 'personal unless the admin shares it');
    await click(share);
    assert.equal(fake.creates().length, 0, 'nothing is saved before Add skill');
    await update(() => { button('Add skill').click(); button('Add skill').click(); });
    await until(update, () => title() === 'Add skills');
    assert.equal(fake.creates().length, 1);
    assert.equal(fake.creates()[0].body.shared, true);
    assert.deepEqual(fake.creates()[0].body.files, measurement);
    assert.deepEqual(fake.assigns().at(-1).body, { skill: 's9', on: true });
    assert.equal(focused(), 'Upload a skill', 'focus returns to Upload a skill');
    await until(update, () => row('site-measurement'));
    assert.match(row('site-measurement').textContent, /Added$/);
    assert.match(dialog().textContent, /1 added to Atlas/);
  }, { url, fetch: fake.fetch });
});

test('Back and Cancel return to the list without saving, and a late check is ignored', async () => {
  const fake = server({ admin: false });
  await reactFixture(async tools => {
    const { button, click, update } = tools;
    await openPanel(tools);
    assert.deepEqual([...dialog().querySelectorAll('.skill-group')].map(node => node.textContent), ['In this workspace · 2', 'Yours · 1']);
    await click(button('Upload a skill'));
    await pasteSkill(tools);
    assert.equal(dialog().querySelectorAll('.skill-share').length, 0, 'only admins can share');
    await click(button('Back'));
    assert.equal(title(), 'Add skills');
    assert.equal(focused(), 'Upload a skill');
    await click(button('Upload a skill'));
    assert.equal(dialog().querySelectorAll('.skill-source').length, 0, 'the draft was discarded');
    await pasteSkill(tools);
    await click(button('Cancel'));
    assert.equal(title(), 'Add skills');
    assert.equal(fake.creates().length, 0);
    assert.equal(fake.assigns().length, 0);
    // A check still on its way when the step is left doesn't come back later.
    let release; fake.holdCheck = new Promise(resolve => { release = resolve; });
    await click(button('Upload a skill'));
    await click(button('or paste a SKILL.md'));
    await tools.change(dialog().querySelector('textarea'), pasted);
    await click(button('Check SKILL.md'));
    await click(button('Back'));
    await click(button('Upload a skill'));
    await update(() => release());
    await update();
    assert.equal(dialog().querySelectorAll('.skill-source').length, 0, 'the late check was ignored');
  }, { url, fetch: fake.fetch });
});

test('a failed upload keeps the step and its review; the retry saves the skill once', async () => {
  const fake = server();
  await reactFixture(async tools => {
    const { button, click, update } = tools;
    await openPanel(tools);
    await click(button('Upload a skill'));
    await pasteSkill(tools);
    fake.failCreate = 1;
    await click(button('Add skill'));
    assert.equal(title(), 'Upload a skill');
    assert.match(dialog().querySelector('.ui-dialog-footer').textContent, /Skills are unavailable right now/);
    assert.ok(dialog().querySelector('.skill-source'), 'the review stays');
    // Saved but not added: the retry only adds it.
    fake.failAssign = 1;
    await click(button('Add skill'));
    assert.match(dialog().querySelector('.ui-dialog-footer').textContent, /at most 20 skills/);
    assert.equal(title(), 'Upload a skill');
    await click(button('Add skill'));
    await until(update, () => title() === 'Add skills');
    assert.equal(fake.creates().length, 2, 'one failed and one saved');
    assert.equal(fake.creates()[0].body.id, fake.creates()[1].body.id, 'the retry reuses the same key');
    assert.deepEqual(fake.assigns().map(write => write.body), [{ skill: 's9', on: true }, { skill: 's9', on: true }]);
    // A problem reading the file stays on the drop step.
    await click(button('Upload a skill'));
    const input = dialog().querySelector('input[type=file][accept]');
    Object.defineProperty(input, 'files', { configurable: true, value: [new tools.dom.window.File([new Uint8Array([1, 2, 3])], 'broken.zip')] });
    await update(() => input.dispatchEvent(new tools.dom.window.Event('change', { bubbles: true })));
    await until(update, () => dialog().querySelector('[role=alert]'));
    assert.equal(title(), 'Upload a skill');
    assert.equal(dialog().querySelectorAll('.skill-source').length, 0);
  }, { url, fetch: fake.fetch });
});

test("the agent's skills list: focus returns when the panel closes, and remove", async () => {
  const fake = server();
  await reactFixture(async tools => {
    const { button, click } = tools;
    await openPanel(tools);
    assert.match(document.body.textContent, /No skills yet/);
    await click(button('Add quote-builder'));
    await click(button('Done'));
    assert.equal(document.querySelectorAll('dialog').length, 0, 'the panel closed');
    assert.equal(focused(), '＋ Add skills', 'Done returns focus to the trigger');
    assert.match(document.querySelector('.agent-skills').textContent, /quote-builderWorkspace skill · version 3 · used 2 times this week/);
    await click(button('＋ Add skills'));
    await click(dialog().querySelector('[aria-label="Close"]'));
    assert.equal(focused(), '＋ Add skills', 'the close button does too');
    await click(button('Actions for quote-builder'));
    await click(button('Remove from this agent'));
    assert.deepEqual(fake.assigns().at(-1).body, { skill: 's1', on: false });
  }, { url, fetch: fake.fetch });
});
