import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

// Agents → Knowledge → working summary (schema 120): who wrote it, and outdated ones.
const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=agents`;
const brief = { objective: { text: 'Plan the Pune warehouse move', sources: ['m1'] },
  decisions: [{ text: 'Move happens in March', sources: ['m9'] }], questions: [], corrections: [] };

test('the owner sees whether a summary was kept up to date automatically or saved by the agent', async () => {
  await reactFixture(async ({ render }) => {
    const { WorkingSummary } = await import('../src/react/agents/saved-context.tsx');
    await render(WorkingSummary, { working: { revision: 3, available: true, made_by: 'automatic', brief } });
    assert.match(document.body.textContent, /Revision 3/);
    assert.match(document.body.textContent, /Kept up to date automatically when this conversation grew long/);
    assert.match(document.body.textContent, /Move happens in MarchEvidence: m9/);
    await render(WorkingSummary, { working: { revision: 2, available: true, made_by: 'agent', brief } });
    assert.match(document.body.textContent, /Saved by the agent/);
    await render(WorkingSummary, { working: { revision: 4, available: false, made_by: 'automatic' } });
    assert.match(document.body.textContent, /Outdated or unavailable/);
  }, { url, fetch: async () => ({ ok: true, json: async () => ({}) }) });
});
