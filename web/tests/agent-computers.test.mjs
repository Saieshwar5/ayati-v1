import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

// Agent computers (designs/agent-computers 1, 7, 8): off until its owner or an admin
// turns it on; sizes with their hourly price; controls for internet and programs;
// commands in Activity; computer time in Usage. The provider is never named.
const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=agents`;
const session = { user: { id: 'owner', email: 'owner@example.test', display_name: 'Ravi Kumar' }, workspaces: [{ id: w, name: 'Mehta Furnishings', role: 'Member' }] };
const response = data => ({ ok: true, json: async () => data });
const offer = { sleep_after_minutes: 5, sizes: [
  { size: 'small', cpu: 1, memory_gb: 2, disk_gb: 10, usd_per_hour: 0.09 },
  { size: 'standard', cpu: 2, memory_gb: 4, disk_gb: 20, usd_per_hour: 0.17 },
] };
const agent = { id: 'atlas', name: 'Atlas', role: 'Orders', model: 'm', audience: 'everyone', revision: 3, settings: { avatar: 'spark', paused: false }, editable: true };
const asleep = { configured: true, settings: { on: true, size: 'standard', programs: 'allow', browser: 'allow', submit: 'ask', internet: 'none', websites: [] },
  computer: { size: 'standard', state: 'asleep', awake_since: null, last_used: 1762600000, created_at: 1762500000, problem: null, in_use: false },
  month: { seconds: 22800, charge_usd: 1.08 }, sleep_after_minutes: 5 };
const computerSwitch = () => [...document.querySelectorAll('label')].find(node => node.textContent === 'Use a computer')?.querySelector('input');
const radio = (group, label) => [...document.querySelector(`[aria-label="${group}"]`).querySelectorAll('label')].find(node => node.textContent === label).querySelector('input');

test('the owner turns the computer on once, sees its state and sizes, and a failed save keeps it off', async () => {
  const saves = []; let fail = true; let release;
  await reactFixture(async ({ render, click, update }) => {
    const { ComputerCard } = await import('../src/react/agents/computer/computer-card.tsx');
    await render(ComputerCard, { agent, offer, saved: async () => {} });
    assert.equal(computerSwitch().checked, false, 'off for an agent saved before computers');
    assert.match(document.body.textContent, /Off: Atlas has no computer tools/);
    await click(computerSwitch());
    assert.match(document.body.textContent, /Not saved/);
    assert.equal(computerSwitch().checked, false);
    fail = false;
    await update(() => { computerSwitch().click(); computerSwitch().click(); });
    assert.equal(saves.length, 2, 'a second click while saving sends nothing');
    await update(() => release());
    assert.deepEqual(saves[1].settings.computer, { on: true, size: 'standard', programs: 'allow', browser: 'allow', submit: 'ask', internet: 'none', websites: [] });
    assert.equal(saves[1].people, undefined, 'turning it on never changes who can talk to it');
    assert.match(document.body.textContent, /Atlas can use its computer\. It is made the first time Atlas needs it/);
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/computer')) return response({ ...asleep, computer: null, month: { seconds: 0, charge_usd: 0 } });
    saves.push(JSON.parse(options.body));
    if (fail) return { ok: false, json: async () => ({ error: 'Not saved' }) };
    await new Promise(resolve => { release = resolve; });
    return response({ ...agent, revision: 4 });
  } });
});

test('an asleep computer shows its size, month and reset, which asks first and resets once', async () => {
  const resets = [];
  await reactFixture(async ({ render, button, click }) => {
    const { ComputerCard } = await import('../src/react/agents/computer/computer-card.tsx');
    const on = { ...agent, settings: { ...agent.settings, computer: asleep.settings } };
    await render(ComputerCard, { agent: on, offer, saved: async () => {} });
    const text = document.body.textContent;
    assert.match(text, /Asleep · costs nothing while asleep/);
    assert.match(text, /Standard2 CPU · 4 GB memory · 20 GB storage/);
    assert.match(text, /\$0\.17 an hour while working/);
    assert.match(text, /This month 6 h 20 min working · \$1\.08/);
    assert.match(text, /Everyone who can talk to Atlas shares this computer/);
    assert.doesNotMatch(text, /e2b|daytona|sandbox/i);
    assert.equal(document.querySelector('fieldset.agent-computer-sizes input[value="standard"]').checked, true);
    assert.equal(document.querySelector('fieldset.agent-computer-sizes legend').textContent, 'Size');
    await click(button('Reset computer'));
    assert.match(document.body.textContent, /Everything on it is deleted/);
    await click([...document.querySelectorAll('dialog button')].find(node => node.textContent === 'Reset computer'));
    assert.equal(resets.length, 1);
    assert.match(document.body.textContent, /Being reset/);
    assert.match(document.body.textContent, /Atlas’s computer is being reset/);
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/computer/reset')) { resets.push(options.body); return response({ ...asleep, computer: { ...asleep.computer, state: 'deleting' } }); }
    return response(asleep);
  } });
});

test('people who cannot change the agent see its computer read-only, and a pending size waits for reset', async () => {
  await reactFixture(async ({ render }) => {
    const { ComputerCard } = await import('../src/react/agents/computer/computer-card.tsx');
    const on = { ...agent, editable: false, settings: { ...agent.settings, computer: { ...asleep.settings, size: 'small' } } };
    await render(ComputerCard, { agent: on, offer, saved: async () => {} });
    assert.equal(computerSwitch().disabled, true);
    assert.equal(document.querySelector('fieldset.agent-computer-sizes').disabled, true);
    assert.equal([...document.querySelectorAll('button')].some(node => node.textContent === 'Reset computer'), false);
    assert.match(document.body.textContent, /Only Atlas’s owner and admins can change its computer/);
    assert.match(document.body.textContent, /Atlas’s computer is Standard\. The new size applies after Reset computer/);
  }, { url, fetch: async () => response(asleep) });
});

test('the Tools tab saves internet and program controls together, with websites checked', async () => {
  const saves = [];
  await reactFixture(async ({ render, button, click, change }) => {
    const { ToolsTab } = await import('../src/react/agents/tools-tab.tsx');
    const on = { ...agent, settings: { ...agent.settings, computer: asleep.settings } };
    await render(ToolsTab, { agent: on, computers: true, saved: async () => {} });
    assert.match(document.body.textContent, /Atlas’s computer can’t reach any website/);
    assert.equal(button('Save computer controls').disabled, true);
    await click(radio('Internet', 'Only these websites'));
    assert.match(document.body.textContent, /No websites yet, so it reaches none/);
    const field = [...document.querySelectorAll('label')].find(node => node.textContent.startsWith('Add a website')).querySelector('input');
    await change(field, 'not a host');
    assert.match(document.body.textContent, /Enter a host name such as portal\.example\.in/);
    assert.equal(button('Add').disabled, true);
    await change(field, 'https://Portal.SharmaTraders.in/login');
    await click(button('Add'));
    await change(field, '*.zoho.in');
    await click(button('Add'));
    await click(radio('Run programs and commands', 'Ask first'));
    assert.match(document.body.textContent, /Each new command asks the person who asked first/);
    assert.match(document.body.textContent, /Atlas asks before pressing a button, sending a form or pressing Enter/);
    await click(radio('Press buttons and send forms', 'Allow'));
    await click(radio('Open and read websites', 'Block'));
    assert.equal(document.querySelector('[aria-label="Press buttons and send forms"]'), null, 'no button control without a browser');
    await click(radio('Open and read websites', 'Ask first'));
    await click(button('Save computer controls'));
    assert.equal(saves.length, 1);
    assert.deepEqual(saves[0].settings.computer, { on: true, size: 'standard', programs: 'ask', browser: 'ask', submit: 'allow', internet: 'listed', websites: ['portal.sharmatraders.in', '*.zoho.in'] });
    assert.match(document.body.textContent, /Computer controls saved/);
  }, { url, fetch: async (path, options = {}) => {
    if (!options.body) return response({ can_manage: true, connections: [], actions: [], execution_available: true });
    saves.push(JSON.parse(options.body));
    return response({ ...agent, revision: 4 });
  } });
});

const directory = { role: 'Member', agents: [], default_model: 'fixture', system_prompt: 'Use only assigned capabilities.', providers: [] };
const field = label => [...document.querySelectorAll('label')].find(node => node.textContent.startsWith(label)).querySelector('input,textarea,select');

test('a new agent gets a computer only when its creator switches it on, with its controls', async () => {
  for (const turnOn of [false, true]) {
    const writes = [];
    await reactFixture(async ({ render, button, click, change }) => {
      const context = await import('../src/workspace-context.ts');
      await context.enterWorkspace();
      const { AgentCreate, newAgent } = await import('../src/react/agents/create/create-flow.tsx');
      await render(AgentCreate, { start: newAgent(directory), computers: offer, close() {}, created: async () => {} });
      await change(field('Name'), 'Iris');
      await click(button('Continue'));
      assert.match(document.body.textContent, /Iris’s own Linux computer for files and programs/);
      assert.equal(computerSwitch().checked, false);
      if (turnOn) {
        await click(computerSwitch());
        await change(field('Size'), 'small');
        assert.match(document.body.textContent, /nothing is charged until then/);
      }
      await click(button('Continue'));
      await click(button('Customer support'));
      for (let next = 0; next < 2; next++) await click(button('Continue'));
      assert.equal(Boolean(document.querySelector('[aria-labelledby="agent-create-computer-title"]')), turnOn);
      if (turnOn) await click(radio('Internet', 'Any website'));
      await click(button('Create agent'));
      assert.equal(writes.length, 1);
      assert.deepEqual(writes[0].settings.computer, turnOn
        ? { on: true, size: 'small', programs: 'allow', browser: 'allow', submit: 'ask', internet: 'any', websites: [] }
        : undefined);
    }, { url, fetch: async (path, options = {}) => {
      if (path.endsWith('/session')) return response(session);
      if (path.endsWith('/chat-directory')) return response({ users: [], agents: [], role: 'Member' });
      if (path.endsWith('/control/available')) return response({ connections: [], execution_available: true });
      const body = JSON.parse(options.body); writes.push(body);
      return response({ ...body, revision: 1 });
    } });
  }
});

test('Activity shows each command with its result, and asking first is not an approval', async () => {
  await reactFixture(async ({ render }) => {
    const { ComputerStep } = await import('../src/react/activity/computer-step.tsx');
    const step = (state, computer) => ({ id: `computer-${state}`, title: 'x', effect: 'computer', state, automatic: true, decision: null, decided_by: null, account: null, arguments: {}, at: 1, computer });
    const { createElement } = await import('react');
    await render(({ steps }) => createElement('ol', null, steps.map(s => createElement(ComputerStep, { key: s.id, step: s }))), { steps: [
      step('completed', { kind: 'command', detail: 'pdftotext may.pdf -', exit_code: 0, output: 'Total 41,300', output_bytes: 12, milliseconds: 1400 }),
      step('failed', { kind: 'command', detail: 'cat missing.txt', exit_code: 1, output: 'No such file', output_bytes: 12, milliseconds: 20 }),
      step('pending', { kind: 'command', detail: 'python3 report.py', exit_code: null, output: '', output_bytes: 0, milliseconds: 0 }),
      { ...step('completed', { kind: 'write', detail: 'shared/prices.csv', exit_code: null, output: '', output_bytes: 9, milliseconds: 0 }), id: 'w', title: 'Saved shared/prices.csv' },
    ] });
    const items = [...document.querySelectorAll('li')];
    assert.equal(items[0].querySelector('code').textContent, 'pdftotext may.pdf -');
    assert.match(items[0].textContent, /Finished · 1\.4 s/);
    assert.equal(items[0].querySelector('pre').textContent, 'Total 41,300');
    assert.match(items[1].textContent, /Finished with exit code 1/);
    assert.equal(items[1].className.includes('problem'), true);
    assert.match(items[2].textContent, /Waiting for the person/);
    assert.equal(items[2].querySelector('button'), null, 'no approval review for a computer step');
    assert.match(items[3].textContent, /Saved shared\/prices\.csv/);
  }, { url });
});

test('Usage adds computer time and charges beside model use, by agent', async () => {
  const totals = { calls: 0, input_tokens: 0, output_tokens: 0, charge_usd: 0 };
  const report = { days: 30, since_ms: 0, scope: 'workspace', totals, by_agent: [], by_model: [], by_person: [], by_day: [],
    computers: { seconds: 7500, charge_usd: 0.35, by_agent: [{ key: 'atlas', name: 'Atlas', seconds: 7500, charge_usd: 0.35 }] }, total_charge_usd: 0.35 };
  await reactFixture(async ({ render }) => {
    const { UsageReportView } = await import('../src/react/usage/usage-report.tsx');
    await render(UsageReportView, { title: 'Usage' });
    const text = document.body.textContent;
    assert.match(text, /Charges\$0\.35/);
    assert.match(text, /Computer time2 h 5 min/);
    const table = [...document.querySelectorAll('table')].find(node => node.querySelector('caption').textContent === 'Computers by agent');
    assert.match(table.textContent, /Atlas2 h 5 min\$0\.35/);
    assert.doesNotMatch(text, /No model or computer use/);
  }, { url, fetch: async () => response(report) });
});

/** A stand-in for the browser's server-sent events. */
class FakeEvents {
  static last;
  constructor(url) { this.url = url; this.listeners = {}; FakeEvents.last = this; }
  addEventListener(name, listen) { this.listeners[name] = listen; }
  close() { this.closed = true; }
  emit(data) { this.onmessage?.({ data: JSON.stringify(data) }); }
}
const withEvents = async run => {
  const previous = globalThis.EventSource;
  globalThis.EventSource = FakeEvents;
  try { await run(); } finally { globalThis.EventSource = previous; }
};

test('the live view is view-only until a hand-over, then sends a person’s clicks and typing', async () => {
  const inputs = [];
  await withEvents(() => reactFixture(async ({ render, button, click, change, update, dom }) => {
    dom.window.HTMLImageElement.prototype.getBoundingClientRect = () => ({ left: 0, top: 0, width: 640, height: 400 });
    const { LiveView } = await import('../src/react/agents/computer/live-view.tsx');
    await render(LiveView, { agent: 'atlas', name: 'Atlas', close() {} });
    assert.match(FakeEvents.last.url, /agent-profiles\/atlas\/computer\/live$/);
    await update(() => FakeEvents.last.emit({ watching: { control: false, reply: 'r1' } }));
    await update(() => FakeEvents.last.emit({ url: 'https://portal.sharmatraders.in/login' }));
    await update(() => FakeEvents.last.emit({ frame: 'AAAA', width: 1280, height: 800 }));
    const picture = document.querySelector('img.computer-live-picture');
    assert.equal(picture.getAttribute('src'), 'data:image/jpeg;base64,AAAA');
    assert.match(document.body.textContent, /View only\. Atlas is working/);
    await update(() => picture.dispatchEvent(new dom.window.MouseEvent('mousedown', { bubbles: true, clientX: 320, clientY: 200 })));
    assert.equal(inputs.length, 0, 'watching sends nothing');

    await update(() => FakeEvents.last.emit({ watching: { control: true, reply: 'r1' } }));
    assert.match(document.body.textContent, /You’re using Atlas’s browser/);
    await update(() => picture.dispatchEvent(new dom.window.MouseEvent('mousedown', { bubbles: true, clientX: 320, clientY: 200 })));
    await update(() => picture.dispatchEvent(new dom.window.MouseEvent('mouseup', { bubbles: true, clientX: 320, clientY: 200 })));
    const field = [...document.querySelectorAll('label')].find(node => node.textContent.startsWith('Type into the page')).querySelector('input');
    await change(field, 'ravi@sharmatraders.in');
    await click(button('Type'));
    await click(button('Tab'));
    assert.deepEqual(inputs, [
      { mouse: { type: 'mousePressed', x: 640, y: 400 } },
      { mouse: { type: 'mouseReleased', x: 640, y: 400 } },
      { text: 'ravi@sharmatraders.in' },
      { key: 'Tab' },
    ]);
    assert.match(document.body.textContent, /Atlas sees nothing while you are in control/);
  }, { url, fetch: async (path, options = {}) => { inputs.push(JSON.parse(options.body)); return response({ sent: true }); } }));
});

test('a hand-over question opens the browser and Done gives it back as the answer', async () => {
  const answers = [];
  const question = { id: 'q1', key: 'browser-hand-over-1a2b3c', revision: 1, state: 'pending', values: null, respondent: 'owner', respondent_name: 'Ravi Kumar', can_respond: true,
    reply: 'r1', chat: 'c1', agent: 'atlas',
    specification: { version: 1, kind: 'single_choice', title: 'Atlas needs you in its browser: sign in', description: 'Sign in to the Sharma Traders portal',
      fallback: '', options: [{ id: 'yes', label: 'Done, give back' }, { id: 'no', label: 'I can’t do it' }], fields: [], allow_other: false } };
  await withEvents(() => reactFixture(async ({ render, button, click, update }) => {
    const context = await import('../src/workspace-context.ts');
    await context.enterWorkspace();
    const { QuestionDialog } = await import('../src/react/questions/dialog.tsx');
    await render(QuestionDialog, { id: 'q1', close() {} });
    await click(button('Take over the browser'));
    assert.match(FakeEvents.last.url, /atlas\/computer\/live$/);
    await update(() => FakeEvents.last.emit({ watching: { control: true, reply: 'r1' } }));
    assert.match(document.body.textContent, /You’re using Atlas’s browser/);
    const done = [...document.querySelectorAll('dialog')].at(-1).querySelectorAll('button');
    await click([...done].find(node => node.textContent === 'Done, give back'));
    assert.equal(answers.length, 1);
    assert.deepEqual(answers[0].values, { choices: ['yes'] });
    assert.equal(answers[0].decision, 'answer');
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (path.includes('/questions/q1') && options.body) { const body = JSON.parse(options.body); answers.push(body); return response({ ...question, state: 'answered', values: body.values }); }
    if (path.includes('/questions/q1')) return response(question);
    return response({});
  } }));
});

test('a hand-over shows the page it needs help with', async () => {
  const question = { id: 'q2', key: 'browser-hand-over-9f', revision: 1, state: 'pending', values: null, respondent: 'owner', respondent_name: 'Ravi Kumar', can_respond: true,
    reply: 'r1', chat: 'c1', agent: 'atlas', screenshot: 'data:image/jpeg;base64,/9j/AAAA',
    specification: { version: 1, kind: 'single_choice', title: 'Atlas needs you in its browser: type a one-time code', description: 'Type the code sent to your phone',
      fallback: '', options: [{ id: 'yes', label: 'Done, give back' }, { id: 'no', label: 'I can’t do it' }], fields: [], allow_other: false } };
  await reactFixture(async ({ render }) => {
    const context = await import('../src/workspace-context.ts');
    await context.enterWorkspace();
    const { QuestionDialog } = await import('../src/react/questions/dialog.tsx');
    await render(QuestionDialog, { id: 'q2', close() {} });
    const picture = document.querySelector('img.question-screenshot');
    assert.equal(picture.getAttribute('src'), 'data:image/jpeg;base64,/9j/AAAA');
    assert.equal(picture.getAttribute('alt'), 'The page Atlas needs help with');
  }, { url, fetch: async path => path.endsWith('/session') ? response(session) : response(question) });
});

test('Needs me lists browser hand-overs apart from questions, to take over', async () => {
  const base = { version: 'pending', at: Date.now(), agent: 'atlas', agent_name: 'Atlas', chat: 'c1', thread: null, conversation_name: 'Orders', archived: false, read: false, reviewed: false };
  const items = [
    { ...base, kind: 'question', id: 'q1', reply: 'r1', title: 'Atlas needs you in its browser: sign in', status: 'browser' },
    { ...base, kind: 'question', id: 'q2', reply: 'r2', title: 'Which supplier?', status: 'pending' },
  ];
  await reactFixture(async ({ render }) => {
    const { NeedsMe } = await import('../src/react/activity/needs-me.tsx');
    await render(NeedsMe, { first: { data: { items, available: true, next_before: null }, error: '', loading: false, refresh: async () => {} }, search: '' });
    const headings = [...document.querySelectorAll('h2.activity-group')].map(node => node.textContent);
    assert.deepEqual(headings, ['In the browser · 1', 'Questions · 1']);
    const browser = document.querySelector('[aria-label="In the browser"]');
    assert.ok([...browser.querySelectorAll('button')].some(node => node.textContent === 'Take over'));
  }, { url });
});
