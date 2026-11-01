import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

// Approvals in Chat follow where they came from (owner decision 2026-11-01): the
// people who may decide get the decision; others see whom it waits for.
const card = async () => (await import('../src/react/approvals/approval-card.tsx'));

test('names read naturally', async () => {
  const { deciderNames } = await card();
  assert.equal(deciderNames(['Priya']), 'Priya');
  assert.equal(deciderNames(['Priya', 'Meera']), 'Priya or Meera');
  assert.equal(deciderNames(['Priya', 'Meera', 'Ravi']), 'Priya, Meera or Ravi');
});

test('a person who may decide gets the decision; others see whom it waits for and the details', async () => {
  const { ApprovalCard, waitingLine } = await card();
  const later = Math.floor(Date.now() / 1000) + 3600;
  for (const [reference, text, action] of [
    [{ id: 'a1', state: 'pending', may_decide: true, waiting_for: ['Kavya Nair'], backup_at: later }, /Waiting for your decision/, 'Review and decide'],
    [{ id: 'a1', state: 'pending', may_decide: false, waiting_for: ['Priya', 'Meera'], backup_at: later }, /Waiting for approval from Priya or Meera\. An admin can step in after/, 'Details'],
    [{ id: 'a1', state: 'pending', may_decide: false, waiting_for: [], backup_at: later }, /Waiting for an admin's approval/, 'Details'],
    [{ id: 'a1', state: 'completed' }, /Completed/, 'Review saved request'],
    // An older server sends no decision facts: the card behaves as before.
    [{ id: 'a1', state: 'pending' }, /Waiting for approval/, 'Review saved request'],
  ]) {
    await reactFixture(async ({ render, button }) => {
      await render(ApprovalCard, { reference });
      assert.match(document.body.textContent, text);
      assert.ok(button(action), action);
      assert.equal(button(action).classList.contains('primary'), action === 'Review and decide');
      assert.equal(button('Review and decide') !== undefined, reference.may_decide === true);
    });
  }
  // Past the backup time, the line doesn't promise a future step-in.
  assert.equal(waitingLine({ state: 'pending', may_decide: false, waiting_for: ['Priya'], backup_at: 1 }), 'Waiting for approval from Priya');
});
