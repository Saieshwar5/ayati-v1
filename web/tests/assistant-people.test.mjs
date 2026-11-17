import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { reactFixture } from './react-fixture.mjs';

// Private agents are assistants now (schema 127): new agents are for people in
// the workspace, and an agent saved as Only me before keeps that choice.
const labels = () => [...document.querySelectorAll('.agent-audience-choice strong')].map(node => node.textContent);
async function show(render, audience) {
  const { PeopleStep } = await import('../src/react/agents/create/people-step.tsx');
  await render(() => createElement(PeopleStep, { name: 'Nova', workspace: 'Northwind', me: 'ravi', members: [{ id: 'ravi', name: 'Ravi' }],
    audience, people: [], setAudience() {}, setPeople() {} }));
}

test('a new agent is for everyone or selected people, never only me', async () => {
  await reactFixture(async ({ render }) => {
    await show(render, 'everyone');
    assert.deepEqual(labels(), ['Everyone in Northwind', 'Selected people']);
  });
});

test('an agent saved as Only me before keeps the choice', async () => {
  await reactFixture(async ({ render }) => {
    await show(render, 'only_me');
    assert.deepEqual(labels(), ['Everyone in Northwind', 'Selected people', 'Only me']);
  });
});
