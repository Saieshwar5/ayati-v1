import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) {
  return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context);
} });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const definition = section => ({ title: 'General panel', purpose: 'Configured data', sections: [section] });
const cases = [
  { kind: 'metric', label: 'Count', value: '12', comparison: { label: 'Previous period', value: -8, unit: '%', favorable: 'decrease' } },
  { kind: 'metrics', items: [{ id: 'a', label: 'Measure A', value: '12' }, { id: 'b', label: 'Measure B', value: '76%' }] },
  { kind: 'table', columns: ['Item', 'Value'], column_types: ['text', 'number'], rows: [['A', '100'], ['B', '2'], ['C', '10']] },
  { kind: 'chart', title: 'Trend', points: [{ label: 'Mon', value: 2 }, { label: 'Tue', value: 4 }] },
  { kind: 'collection', items: [{ id: 'a', title: '<img src=x onerror=alert(1)>', description: 'Description', status: 'Active' }] },
  { kind: 'progress', items: [{ id: 'a', label: 'Measure A', actual: 72, target: 100 }] },
  { kind: 'timeline', events: [{ id: 'a', at_ms: 2000, label: 'Later' }, { id: 'b', at_ms: 1000, label: 'Earlier' }] },
  { kind: 'text', heading: 'Context', body: '<script>bad()</script>\nA supporting point.' },
];

test('all general families render inert data in React and adopted Home without fetching or executing agent UI', async () => {
  let reads = 0;
  await reactFixture(async ({ render }) => {
    const { PanelContent } = await import('../src/react/home-panels/snapshot.tsx');
    const { renderPanel } = await import('../src/panel-renderer.ts');
    for (const section of cases) {
      await render(PanelContent, { definition: definition(section) });
      const compatibility = renderPanel(definition(section));
      assert.equal(document.querySelectorAll('script,img,iframe').length, 0);
      assert.equal(compatibility.querySelectorAll('script,img,iframe').length, 0);
      assert.ok(document.querySelector(`.panel-${section.kind}`));
      assert.ok(compatibility.querySelector(`.panel-${section.kind}`));
      if (section.kind === 'metric') assert.equal(document.querySelector('.panel-comparison').dataset.sentiment, 'favorable');
      if (section.kind === 'timeline') assert.equal(document.querySelector('.panel-timeline strong').textContent, 'Later');
    }
    assert.equal(reads, 0);
  }, { fetch: async () => { reads++; throw new Error('Rendering may not start agent work'); } });
});

test('typed table sorting is numeric, keyboard-accessible and preserves the active sort when values refresh', async () => {
  await reactFixture(async ({ render, button, click }) => {
    const { PanelContent } = await import('../src/react/home-panels/snapshot.tsx');
    const value = definition(cases[2]);
    await render(PanelContent, { definition: value });
    await click(button('Value'));
    assert.deepEqual([...document.querySelectorAll('tbody tr')].map(r => r.cells[1].textContent), ['2', '10', '100']);
    assert.equal(document.querySelectorAll('th')[1].getAttribute('aria-sort'), 'ascending');
    await render(PanelContent, { definition: definition({ ...cases[2], rows: [['A', '1'], ['B', '20'], ['C', '10']] }) });
    assert.deepEqual([...document.querySelectorAll('tbody tr')].map(r => r.cells[1].textContent), ['1', '10', '20']);
    assert.equal(document.activeElement.textContent, 'Value ↑');
    await click(button('Value ↑'));
    assert.deepEqual([...document.querySelectorAll('tbody tr')].map(r => r.cells[1].textContent), ['20', '10', '1']);
  });
});

test('five chart variants keep numeric geometry finite and provide full accessible values', async () => {
  await reactFixture(async ({ render }) => {
    const { PanelContent } = await import('../src/react/home-panels/snapshot.tsx');
    for (const variant of ['line', 'bar', 'area', 'stacked_bar', 'donut']) {
      const section = { kind: 'chart', title: 'Measure', variant, unit: '%',
        points: [{ label: 'A very long category label', value: 2 }, { label: 'B', value: 4 }] };
      await render(PanelContent, { definition: definition(section) });
      assert.ok(document.querySelector('svg[role=img]'));
      assert.match(document.querySelector('details').textContent, /A very long category label/);
      assert.match(document.querySelector('details').textContent, /2%/);
      assert.doesNotMatch(document.querySelector('svg').outerHTML, /NaN|Infinity/);
    }
    for (const variant of ['line', 'bar', 'area']) {
      await render(PanelContent, { definition: definition({ kind: 'chart', title: 'Extreme', variant,
        points: [{ label: 'Low', value: -Number.MAX_VALUE }, { label: 'High', value: Number.MAX_VALUE }] }) });
      assert.doesNotMatch(document.querySelector('svg').outerHTML, /NaN|Infinity/);
    }
  });
});

test('empty records and zero-total donut show explicit empty states and progress over target stays bounded', async () => {
  await reactFixture(async ({ render }) => {
    const { PanelContent } = await import('../src/react/home-panels/snapshot.tsx');
    for (const [section, message] of [
      [{ kind: 'chart', title: 'Empty', points: [] }, 'No chart data yet.'],
      [{ kind: 'chart', title: 'Zero', variant: 'donut', points: [{ label: 'A', value: 0 }] }, 'No positive values to plot.'],
      [{ kind: 'collection', items: [] }, 'No items yet.'],
      [{ kind: 'timeline', events: [] }, 'No events yet.'],
      [{ kind: 'table', columns: ['Name'], rows: [] }, 'No records yet.'],
    ]) {
      await render(PanelContent, { definition: definition(section) });
      assert.match(document.body.textContent, new RegExp(message));
      assert.equal(document.querySelector('svg'), null);
    }
    await render(PanelContent, { definition: definition({ kind: 'progress', items: [{ id: 'a', label: 'Goal', actual: 125, target: 100 }] }) });
    const track = document.querySelector('[role=progressbar]');
    assert.equal(track.getAttribute('aria-valuenow'), '100');
    assert.match(track.getAttribute('aria-valuetext'), /125 actual/);
    assert.equal(track.firstChild.style.width, '100%');
    assert.match(document.body.textContent, /125%/);
  });
});

test('small nonzero measures never become a displayed zero and chart details retain the numeric value', async () => {
  const { formatValue, exactValue } = await import('../src/panel-data.ts');
  assert.notEqual(formatValue(.000000001, '%'), '0%');
  assert.equal(exactValue(.123456789, '%'), '0.123456789%');
  await reactFixture(async ({ render }) => {
    const { PanelContent } = await import('../src/react/home-panels/snapshot.tsx');
    await render(PanelContent, { definition: definition({ kind: 'chart', title: 'Precise measure', unit: '%',
      points: [{ label: 'A', value: .123456789 }] }) });
    assert.equal(document.querySelector('details td').textContent, '0.123456789%');
    assert.equal(document.querySelector('circle title').textContent, 'A: 0.123456789%');
  });
});

test('choosing a general family or chart variant creates no work until explicit submission and exact retries retain the choice', async () => {
  const w = '11111111-1111-4111-8111-111111111111', writes = [];
  const response = (value, status=200) => ({ ok: status < 400, status, json: async () => value });
  await reactFixture(async ({ render, change, submit }) => {
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { AskPanel } = await import('../src/react/home-panels/create.tsx');
    await render(AskPanel, { close: () => {} });
    const byLabel = label => document.getElementById([...document.querySelectorAll('label')].find(l => l.textContent === label).htmlFor);
    await change(byLabel('Agent'), 'agent'); await change(byLabel('Conversation'), 'chat');
    await change(byLabel('Panel type'), 'chart'); await change(byLabel('Chart style'), 'stacked_bar');
    await change(byLabel('What should the panel show?'), 'Use my selected data.');
    assert.equal(writes.length, 0);
    await submit(document.querySelector('form'));
    await submit(document.querySelector('form'));
    assert.equal(writes.length, 2);
    assert.equal(writes[0].id, writes[1].id);
    assert.match(writes[0].content, /Requested UI family: chart; chart variant: stacked_bar/);
    assert.equal(byLabel('Chart style').value, 'stacked_bar');
    await change(byLabel('Panel type'), 'progress'); await submit(document.querySelector('form'));
    assert.notEqual(writes[2].id, writes[1].id);
    assert.match(writes[2].content, /Requested UI family: progress/);
  }, { url: `http://localhost/w/${w}/?view=home`, fetch: async (path, options={}) => {
    if (options.method === 'POST') { writes.push(JSON.parse(options.body)); return response({ error: 'Keep the request for retry' }, 503); }
    if (path.endsWith('/session')) return response({ user: { id: 'owner' }, workspaces: [{ id: w, name: 'Work', role: 'Owner' }] });
    return path.endsWith('/chat-directory') ? response({ agents: [{ id: 'agent', name: 'Nova', scope: 'workspace' }], users: [] })
      : response({ chats: [{ id: 'chat', name: 'Work', agents: ['agent'], users: [] }] });
  } });
});
