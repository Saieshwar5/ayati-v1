import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

// Each conversation is its own room; only owners and admins are told they may
// ask an agent about its work everywhere (runtime/owner-view.md).
test('a direct chat with an agent says who may ask about its work in every conversation', async () => {
  await reactFixture(async () => {
    const { agentChatNote } = await import('../src/react/chat/model.ts');
    assert.equal(agentChatNote('Atlas', true, false),
      'Only you read this chat. As one of its owners you can ask Atlas about its work in every conversation: who asked, what it did and what is waiting.');
    assert.match(agentChatNote('Atlas', false, true), /^Only you read this chat\. As an admin you can ask Atlas/);
    assert.match(agentChatNote('Atlas', true, true), /As one of its owners/);
    assert.equal(agentChatNote('Atlas', false, false),
      'Atlas keeps each conversation separate. Activity with Atlas is visible to its owners and admins.');
  }, { url: 'http://localhost/w/11111111-1111-4111-8111-111111111111/?view=chat', fetch: async () => ({ ok: true, json: async () => ({}) }) });
});
