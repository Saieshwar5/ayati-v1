import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

// "Did this go out?" (designs/connector-screens 4): settling a write the app never confirmed.
const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=activity&tab=needs`;
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const now = Math.floor(Date.now() / 1000);
const action = (extra = {}) => ({ id: 'act-1', connection: 'c-gmail', operation: 'gmail.send_email', title: 'Send an email', effect: 'write',
  arguments: { to: 'priya@sharma.test', subject: 'May invoice INV-2214' }, state: 'uncertain', result: null, chat: 'orders', thread: null, job: 'j', approver: 'riya',
  created: now - 3600, updated: now, agent: 'atlas', agent_name: 'Atlas', account_name: 'Gmail', approver_name: 'Riya', can_decide: false, decision: 'approve',
  requester_name: 'Arjun Mehta', conversation_name: '#orders', decided_by_name: 'Riya Kapoor',
  settle: { can_settle: true, checks: [{ at: now - 1800, result: 'unknown' }, { at: now - 1500, result: 'unknown' }] }, ...extra });
const settled = (happened, by_name, note = '') => action({ state: happened ? 'completed' : 'not_sent', settle: null, settled: { by: 'x', by_name, at: now, happened, note } });
async function open(render) {
  const { OutcomeDialog } = await import('../src/react/approvals/outcome-dialog.tsx');
  await render(OutcomeDialog, { id: 'act-1', close() {} });
  return document.querySelector('dialog[open]');
}
const radio = label => [...document.querySelectorAll('.outcome-answers label')].find(node => node.textContent.startsWith(label)).querySelector('input');

test('Either answer saves once with the note, and the dialog then says who answered', async () => {
  for (const [label, happened] of [['It did not go through', false], ['It went through', true]]) {
    const writes = [];
    await reactFixture(async ({ render, update, change, button }) => {
      const dialog = await open(render);
      assert.equal(dialog.querySelector('h2').textContent, 'Did this go out?');
      assert.match(dialog.textContent, /Gmail didn’t confirm this action\. Cuckoo has not tried it again/);
      assert.match(dialog.textContent, /ActionSend an email · Gmail.*topriya@sharma\.test.*subjectMay invoice INV-2214/);
      assert.match(dialog.textContent, /Asked byArjun Mehta in #orders, .* · approved by Riya Kapoor/);
      assert.match(dialog.textContent, /Looked for it: Gmail didn’t answer.*Looked for it: Gmail didn’t answer.*Look in Gmail yourself, then answer below\./);
      assert.equal(button('Save answer').disabled, true, 'an answer is chosen first');
      await update(() => radio(label).click());
      await change(dialog.querySelector('input[aria-label=Note]'), ' Checked Sent ');
      await update(() => { button('Save answer').click(); button('Save answer').click(); });
      assert.equal(writes.length, 1, 'one save per click burst');
      assert.ok(writes[0].path.endsWith('/control/actions/act-1/outcome'));
      assert.deepEqual(writes[0].body, { happened, note: 'Checked Sent' });
      assert.match(dialog.textContent, new RegExp(`Riya Kapoor answered today at .*: ${happened ? 'it went through' : 'it did not go through'}\\.`));
      assert.match(dialog.textContent, /Note: “Checked Sent”/);
      assert.equal(dialog.querySelector('.outcome-answers'), null);
    }, { url, fetch: async (path, options = {}) => {
      if (options.body) { const body = JSON.parse(options.body); writes.push({ path, body }); return response(settled(body.happened, 'Riya Kapoor', body.note)); }
      return response(action());
    } });
  }
});

test('A failed save keeps the answer and note; when someone else answered first, theirs is shown', async () => {
  const writes = []; let attempt = 0;
  await reactFixture(async ({ render, update, change, click, button }) => {
    const dialog = await open(render);
    await update(() => radio('It went through').click());
    await change(dialog.querySelector('input[aria-label=Note]'), 'Found in Sent');
    await click(button('Save answer'));
    assert.match(dialog.querySelector('[role=status]').textContent, /Gmail is unavailable/);
    assert.equal(radio('It went through').checked, true);
    assert.equal(dialog.querySelector('input[aria-label=Note]').value, 'Found in Sent', 'the note is kept');
    await click(button('Save answer'));
    assert.deepEqual(writes[0], writes[1]);
    assert.match(dialog.querySelector('.ui-dialog-footer [role=status]').textContent, /Someone else answered first, so your answer was not saved\./);
    assert.match(dialog.textContent, /Meera Iyer answered today at .*: it did not go through\./);
    assert.equal(button('Save answer'), undefined); assert.ok(button('Close'));
  }, { url, fetch: async (path, options = {}) => {
    if (options.body) {
      writes.push(JSON.parse(options.body));
      return ++attempt === 1 ? response({ error: 'Gmail is unavailable' }, 503) : response({ error: 'Someone else already answered' }, 409);
    }
    return response(attempt >= 2 ? settled(false, 'Meera Iyer') : action());
  } });
});

test('People who cannot answer see the action and who can', async () => {
  await reactFixture(async ({ render, button }) => {
    const dialog = await open(render);
    assert.equal(dialog.querySelector('.outcome-answers'), null);
    assert.match(dialog.textContent, /Atlas’s managers and workspace admins can answer this\./);
    assert.equal(button('Save answer'), undefined);
  }, { url, fetch: async () => response(action({ settle: { can_settle: false, checks: [] } })) });
});

test('Check in Activity’s Needs review opens the same dialog', async () => {
  const reads = [];
  await reactFixture(async ({ render, click, button }) => {
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { ActivityPage } = await import('../src/react/activity/activity-page.tsx');
    await render(ActivityPage, { href: url });
    const review = document.querySelector('[aria-label="Needs review"]');
    assert.match(review.textContent, /Send an email.*didn’t confirm it/);
    await click(button('Check'));
    assert.equal(document.querySelector('dialog[open] h2').textContent, 'Did this go out?');
    assert.ok(reads.some(path => path.endsWith('/control/actions/act-1')));
  }, { url, fetch: async (path, options = {}) => {
    reads.push(path);
    if (path.endsWith('/session')) return response({ user: { id: 'riya', email: 'riya@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Admin' }] });
    if (path.includes('/attention') && options.body) return response({ saved: true });
    if (path.includes('/attention')) return response({ available: true, next_before: null, items: [{ kind: 'approval', id: 'act-1', version: 'uncertain', title: 'Send an email',
      status: 'uncertain', at: Date.now(), agent: 'atlas', agent_name: 'Atlas', chat: 'orders', thread: null, reply: 'r1', conversation_name: '#orders', archived: false, read: false, reviewed: false }] });
    if (path.endsWith('/control/actions/act-1')) return response(action());
    return response({ agents: [], running: [], away: { since: Date.now(), totals: {} } });
  } });
});
