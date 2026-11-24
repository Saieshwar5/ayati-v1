// Agent tasks and calendar (designs/agent-tasks): the line under a reply with the next
// run and Undo, a task's rule, runs and notes with Pause and Skip next, Activity ›
// Calendar for a week in India time, and the workspace's working days and holidays.
import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const w = '11111111-1111-4111-8111-111111111111';
const session = { user: { id: 'meera', email: 'meera@example.test', display_name: 'Meera Iyer' }, workspaces: [{ id: w, name: 'Store', role: 'Admin' }] };
const response = data => ({ ok: true, status: 200, json: async () => data });
const settle = update => update(async () => { await new Promise(resolve => setTimeout(resolve, 20)); });
const enter = async () => { const context = await import('../src/workspace-context.ts'); await context.enterWorkspace(); };

test('a task made in chat shows its next run, and only its person can Undo it, once', async () => {
  const writes = [];
  const task = { id: 't1', title: 'Chase invoice 311', kind: 'commitment', status: 'scheduled', requester: 'meera', repeats: true, next_text: 'Thu 26 Nov, 10:00' };
  await reactFixture(async ({ render, update, button }) => {
    await enter();
    const { TaskChip } = await import('../src/react/chat/task-chip.tsx');
    await render(TaskChip, { task });
    const line = document.querySelector('.task-line');
    assert.match(line.textContent, /Chase invoice 311 · next Thu 26 Nov, 10:00/);
    assert.equal(line.querySelector('a').getAttribute('href'), `/w/${w}/?view=activity&tab=open&task=t1`);
    await update(() => { const undo = button('Undo'); undo.click(); undo.click(); });
    await settle(update);
    const closes = writes.filter(x => x.path.endsWith('/activity/tasks/t1/close'));
    assert.equal(closes.length, 1, 'a double click undoes once');
    assert.deepEqual(closes[0].body, { outcome: 'cancelled' });
    assert.match(document.querySelector('.task-line').textContent, /undone/);
    assert.equal(button('Undo'), undefined);
    await render(TaskChip, { task: { ...task, id: 't2', requester: 'arjun' } });
    assert.equal(button('Undo'), undefined, 'someone else sees View only');
  }, { url: `http://localhost/w/${w}/`, fetch: async (path, init) => {
    if (path.endsWith('/session')) return response(session);
    if (init?.method === 'POST') { writes.push({ path, body: JSON.parse(init.body) }); return response({ id: 't1', outcome: 'cancelled' }); }
    return response({});
  } });
});

test('a repeating task shows its rule, runs and notes; Pause and Skip next each send one request', async () => {
  const writes = [];
  const now = Date.now();
  let paused = false;
  const detail = () => ({ id: 't1', title: 'Chase invoice 311', kind: 'commitment', origin: 'agent', status: paused ? 'paused' : 'scheduled', done_when: 'Invoice 311 is paid',
    waiting_on: null, agent: 'atlas', agent_name: 'Atlas', agent_avatar: null, requester: 'meera', requester_name: 'Meera Iyer', chat: 'c1', conversation_name: 'Atlas',
    conversation_kind: 'direct', created_ms: now - 86400000, start_ms: null, due_ms: null, closed_ms: null, outcome: null, result: null, steps: 2,
    next_wakeup_ms: now + 86400000, blocked_wakeup: null, may_close: true, requests: [], repeats: true, paused,
    lifetime: { rule_text: 'Every 2 days at 10:00, until Invoice 311 is paid, gives up after 5 runs', next_ms: paused ? null : now + 86400000,
      next_text: paused ? null : 'Thu 26 Nov, 10:00', timezone: 'Asia/Kolkata', runs: 1, max_runs: 5, paused },
    runs: [{ at: now - 3600000, kind: 'repeat', run: 1, detail: '', reply: 'r1', state: 'done' }, { at: now - 7200000, kind: 'repeat', run: null, detail: 'Skipped: the last run was still going.', reply: null, state: 'skipped' }],
    notes: [{ note: 'Sent reminder 1; no reply yet', created_ms: now - 3600000 }], paused_reason: paused ? 'Paused by Meera Iyer.' : null });
  await reactFixture(async ({ render, update, button }) => {
    await enter();
    const { TaskDetails } = await import('../src/react/activity/task-details.tsx');
    await render(TaskDetails, { id: 't1', close: () => {} });
    await settle(update);
    assert.match(document.querySelector('.activity-task-rule').textContent, /Every 2 days at 10:00, until Invoice 311 is paid, gives up after 5 runs/);
    const facts = document.querySelector('.activity-facts').textContent;
    assert.match(facts, /Next runThu 26 Nov, 10:00 · India time/);
    assert.match(facts, /Runs so far1 of 5/);
    const runs = [...document.querySelectorAll('.activity-steps li')].map(li => li.textContent);
    assert.match(runs[0], /Run 1 · Done/);
    assert.match(runs[1], /Run · Skipped.*still going/);
    assert.match(document.querySelector('.activity-task-notes').textContent, /Sent reminder 1; no reply yet/);
    assert.match(document.body.textContent, /tell Atlas in chat/);
    assert.equal(button('Mark done'), undefined, 'a repeating task is ended, not marked done');
    await update(() => { const skip = button('Skip next run'); skip.click(); skip.click(); });
    await settle(update);
    assert.equal(writes.filter(x => x.path.endsWith('/tasks/t1/skip')).length, 1);
    await update(() => button('Pause').click());
    await settle(update);
    assert.deepEqual(writes.find(x => x.path.endsWith('/tasks/t1/pause')).body, { paused: true });
    assert.ok(button('Resume'), 'a paused task can be resumed');
    assert.match(document.body.textContent, /Paused: Paused by Meera Iyer\./);
    assert.ok(button('End task'));
  }, { url: `http://localhost/w/${w}/`, fetch: async (path, init) => {
    if (path.endsWith('/session')) return response(session);
    if (init?.method === 'POST') {
      writes.push({ path, body: JSON.parse(init.body) });
      if (path.endsWith('/pause')) paused = true;
      return response(detail().lifetime);
    }
    if (path.endsWith('/activity/tasks/t1')) return response(detail());
    return response({});
  } });
});

test('an event trigger task pauses here, resumes on the agent page and is ended, not marked done or "not a task"', async () => {
  const writes = [];
  let state = 'enabled';
  const detail = () => ({ id: 't9', title: 'New orders', kind: 'commitment', origin: 'event', status: state === 'enabled' ? 'open' : 'paused', done_when: null,
    waiting_on: null, agent: 'atlas', agent_name: 'Atlas', agent_avatar: null, requester: 'meera', requester_name: 'Meera Iyer', chat: 'c1', conversation_name: 'orders',
    conversation_kind: 'public', created_ms: Date.now() - 86400000, start_ms: null, due_ms: null, closed_ms: null, outcome: null, result: null, steps: 2,
    next_wakeup_ms: null, blocked_wakeup: null, may_close: true, requests: [], repeats: false, paused: state !== 'enabled',
    lifetime: { rule_text: null, next_ms: null, next_text: null, timezone: 'Asia/Kolkata', runs: 2, max_runs: null, paused: state !== 'enabled' },
    runs: [{ at: Date.now() - 3600000, kind: 'event', run: 2, detail: '', reply: 'r2', state: 'no_change' }], notes: [],
    paused_reason: state === 'enabled' ? null : 'Paused by Meera Iyer.', trigger: { id: 'e1', state, detail: '', source: 'Order changed' } });
  await reactFixture(async ({ render, update, button }) => {
    await enter();
    const { TaskDetails } = await import('../src/react/activity/task-details.tsx');
    await render(TaskDetails, { id: 't9', close: () => {} });
    await settle(update);
    assert.match(document.querySelector('.activity-task-rule').textContent, /When Order changed sends an event/);
    assert.match(document.querySelector('.activity-facts').textContent, /Runs so far2/);
    assert.match(document.querySelector('.activity-steps li').textContent, /Run 2 · No change/);
    assert.equal(button('Mark done'), undefined);
    assert.equal(button('Not a task'), undefined, 'a trigger is ended, not dismissed');
    assert.ok(button('End task'));
    await update(() => button('Pause').click());
    await settle(update);
    assert.deepEqual(writes.find(x => x.path.endsWith('/tasks/t9/pause')).body, { paused: true });
    assert.equal(button('Resume'), undefined, 'a trigger resumes where its access is reviewed');
    const resume = [...document.querySelectorAll('a')].find(a => a.textContent === "Resume on the agent's page");
    assert.match(resume.getAttribute('href'), /selected=atlas&section=automations/);
  }, { url: `http://localhost/w/${w}/`, fetch: async (path, init) => {
    if (path.endsWith('/session')) return response(session);
    if (init?.method === 'POST') { writes.push({ path, body: JSON.parse(init.body) }); state = 'paused'; return response(detail().lifetime); }
    if (path.endsWith('/activity/tasks/t9')) return response(detail());
    return response({});
  } });
});

test('the Calendar shows a week in India time, what has no time beside it, and keeps the list for old links', async () => {
  const reads = [];
  const ist = (local) => Date.parse(`${local}:00+05:30`);
  const start = (() => { const d = new Date(); d.setHours(0, 0, 0, 0); d.setDate(d.getDate() - (d.getDay() + 6) % 7); return d.getTime(); })();
  const day = new Date(start + 2 * 86400000 + 43200000);
  const key = new Intl.DateTimeFormat('en-CA', { timeZone: 'Asia/Kolkata' }).format(day);
  const items = [
    { kind: 'run', task: 't1', title: 'Morning brief', agent: 'atlas', agent_name: 'Atlas', at: ist(`${key}T09:00`), state: 'done', detail: '', reply: 'r1' },
    { kind: 'run', task: 't2', title: 'Stock check', agent: 'mira', agent_name: 'Mira', at: ist(`${key}T11:00`), state: 'no_change', detail: '', reply: 'r2' },
    { kind: 'planned', task: 't3', title: 'Chase invoice 311', agent: 'atlas', agent_name: 'Atlas', at: ist(`${key}T15:30`), run_kind: 'repeat' },
  ];
  const untimed = [{ kind: 'trigger', trigger: 'e1', title: 'New order in Zoho Books', agent: 'atlas', agent_name: 'Atlas', detail: 'Zoho Books' },
    { kind: 'paused', task: 't4', title: 'Month-end GST summary', agent: 'atlas', agent_name: 'Atlas', detail: 'Cuckoo paused it after 3 failed runs in a row.' }];
  await reactFixture(async ({ render, update, button }) => {
    await enter();
    const { ActivityPage } = await import('../src/react/activity/activity-page.tsx');
    await render(ActivityPage, { href: `http://localhost/w/${w}/?view=activity&tab=planned` });
    await settle(update);
    assert.equal(document.querySelector('[role=tab][aria-selected=true]').textContent, 'Calendar');
    const first = reads.find(path => path.includes('/activity/calendar?'));
    assert.ok(first, 'the week is read from the calendar');
    assert.match(document.querySelector('.activity-calendar-title').textContent, /India time/);
    const days = [...document.querySelectorAll('.activity-day')];
    assert.equal(days.length, 7);
    const entries = [...days[2].querySelectorAll('.activity-entry')].map(a => a.textContent);
    assert.deepEqual(entries, ['09:00 · DoneMorning briefAtlas', '11:00 · No changeStock checkMira', '15:30Chase invoice 311Atlas']);
    assert.equal(days[2].querySelector('.activity-entry').getAttribute('href'), `/w/${w}/?view=activity&tab=open&task=t1`);
    const side = document.querySelector('.activity-untimed').textContent;
    assert.match(side, /New order in Zoho Books.*When something happens/);
    assert.match(side, /Month-end GST summary.*Paused.*3 failed runs/);
    await update(() => button('Next week').click());
    await settle(update);
    const next = reads.filter(path => path.includes('/activity/calendar?')).at(-1);
    assert.notEqual(next, first, 'the next week asks for its own window');
    await update(() => button('List').click());
    await settle(update);
    assert.ok(reads.some(path => path.includes('/activity/upcoming')), 'the list keeps Pause for triggers and follow-ups');
  }, { url: `http://localhost/w/${w}/?view=activity&tab=planned`, fetch: async path => {
    reads.push(path);
    if (path.endsWith('/session')) return response(session);
    if (path.includes('/attention')) return response({ available: true, items: [], next_before: null });
    if (path.includes('/activity/calendar')) return response({ from: 0, to: 0, now: Date.now(), timezone: 'Asia/Kolkata', items, untimed });
    if (path.includes('/activity/upcoming')) return response({ items: [] });
    return response({});
  } });
});

test('admins choose working days and holidays; one Save sends them together', async () => {
  const writes = [];
  await reactFixture(async ({ render, update, change, submit, button }) => {
    await enter();
    const { WorkspaceCalendarSettings } = await import('../src/react/settings/workspace-calendar.tsx');
    await render(WorkspaceCalendarSettings);
    await settle(update);
    const boxes = [...document.querySelectorAll('.settings-day input')];
    assert.deepEqual(boxes.map(b => b.checked), [true, true, true, true, true, true, false], 'Monday to Saturday');
    await update(() => boxes[5].click());
    const [date, name] = document.querySelectorAll('.settings-holiday-add input');
    await change(date, '2026-11-09');
    await change(name, 'Diwali');
    await update(() => button('Add holiday').click());
    assert.match(document.querySelector('.settings-holidays').textContent, /Diwali 2026-11-09/);
    await change(document.querySelector('.settings-calendar select'), 'skip');
    const form = document.querySelector('.settings-calendar');
    await update(() => { submit(form); submit(form); });
    await settle(update);
    const saves = writes.filter(x => x.path.endsWith('/settings/calendar'));
    assert.equal(saves.length, 1, 'saved once');
    assert.deepEqual(saves[0].body, { working_days: [1, 2, 3, 4, 5], holiday_rule: 'skip', holidays: [{ day: '2026-11-09', name: 'Diwali' }] });
    assert.match(document.querySelector('.settings-calendar [role=status]').textContent, /Saved\./);
  }, { url: `http://localhost/w/${w}/?view=settings`, fetch: async (path, init) => {
    if (path.endsWith('/session')) return response(session);
    if (init?.method === 'POST') { const body = JSON.parse(init.body); writes.push({ path, body }); return response({ timezone: 'Asia/Kolkata', ...body }); }
    if (path.endsWith('/settings/calendar')) return response({ timezone: 'Asia/Kolkata', working_days: [1, 2, 3, 4, 5, 6], holiday_rule: 'next', holidays: [], editable: true });
    return response({});
  } });
});
