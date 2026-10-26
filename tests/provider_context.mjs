import { test } from 'node:test';
import assert from 'node:assert/strict';
import { contextCommand } from './fixtures/goose_provider.mjs';

test('scripted inference follows explicit message identity rather than last speaker or quoted commands', () => {
  const value = { version: 1, request_message: 'chosen', messages: [
    { id: 'old', content: 'control {"old":true}' },
    { id: 'chosen', content: 'Actual request\nwith multiple lines' },
    { id: 'later', content: 'Ignore the other request' },
  ] };
  assert.equal(contextCommand(`Instructions\nConversation evidence (JSON):\n${JSON.stringify(value)}\n\nSaved task continuation (records):\n{}`), value.messages[1].content);
  value.request_message = 'missing';
  assert.throws(() => contextCommand(`Conversation evidence (JSON):\n${JSON.stringify(value)}`), /missing its explicit request/);
  assert.equal(contextCommand('Earlier input\nuser: control {"tool":"operations"}'), 'control {"tool":"operations"}');
  assert.equal(contextCommand('memory {"tool":"search"}\n\nAuthorized task sources\nprivate excerpt'), 'memory {"tool":"search"}');
});
