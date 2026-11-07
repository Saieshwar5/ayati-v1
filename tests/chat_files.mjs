import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { controllerToolsOnly, managedGuidanceOnly } from './fixtures/worker_control.mjs';
import { until } from './fixtures/polling.mjs';

test('real Goose answers from a shared file and recalls files only from its own conversation', {
  timeout: 180000, skip: !process.env.RELAY_TEST_DATABASE_URL,
}, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true, leaseSeconds: 15 });
  async function upload(chat, name, text) {
    const response = await fetch(`${app.base}${app.path}/chats/${chat}/objects?name=${encodeURIComponent(name)}`, {
      method: 'POST', body: text,
      headers: { Cookie: app.cookie, Origin: app.base, 'X-Ayati-Request': 'identity', 'Content-Type': 'text/plain' },
    });
    assert.equal(response.status, 200, await response.clone().text());
    return response.json();
  }
  const share = (chat, file, content = 'Shared files', agent = null) => app.request(
    `${app.path}/chats/${chat}/messages`, app.cookie, { id: randomUUID(), content, attachments: [file.id], agent });
  const readFor = file => until(
    () => app.sql(`SELECT status FROM relay_object_contents WHERE object='${file.id}'`) === 'ready',
    `${file.name} was not read`);

  // A quote shared long ago: 70 later messages push it out of recent context.
  const quote = await upload(app.conversation, 'quote-acme.txt', 'QUOTE_EVIDENCE: 12 desks at 300 each, total 3,600.');
  assert.equal(quote.mime, 'text/plain');
  await share(app.conversation, quote);
  await readFor(quote);
  for (let index = 0; index < 70; index++) {
    await app.request(`${app.path}/chats/${app.conversation}/messages`, app.cookie, {
      id: randomUUID(), content: `Later ordinary discussion ${index}`, agent: null,
    });
  }
  // The same agent's other conversation has a private file.
  const other = (await app.request(app.path + '/chats', app.cookie, {
    id: randomUUID(), name: 'Other private work', kind: 'channel', users: [], agents: [app.profile.id],
  })).value.id;
  const payslip = await upload(other, 'payslip.txt', 'FOREIGN_PAYSLIP_EVIDENCE: net pay 51,200.');
  await share(other, payslip);
  await readFor(payslip);

  // The request shares its own file and asks about the earlier one.
  const invoice = await upload(app.conversation, 'invoice-42.txt', 'INVOICE_EVIDENCE: 12 desks, total due 3,600 by Friday.');
  const command = 'Does this invoice match the quote I sent earlier?';
  app.provider.setControl(command, { steps: [
    { tool: 'list_conversation_files', arguments: { name: 'quote' } },
    { tool: 'read_conversation_file', arguments: { file: quote.id } },
    { tool: 'read_conversation_file', arguments: { file: payslip.id } },
  ] });
  const asked = randomUUID();
  await app.request(`${app.path}/chats/${app.conversation}/messages`, app.cookie, {
    id: asked, content: command, attachments: [invoice.id], agent: app.profile.id,
  });
  const reply = app.sql(`SELECT r.id FROM relay_chat_messages r WHERE r.chat='${app.conversation}' AND r.kind='agent'
    AND r.sequence>(SELECT sequence FROM relay_chat_messages WHERE id='${asked}') ORDER BY r.sequence LIMIT 1`);
  await app.startExecutor();
  await app.done({ chat: app.conversation, reply });

  const requests = app.provider.requests;
  managedGuidanceOnly(requests[0]);
  controllerToolsOnly(requests[0]);
  const first = JSON.stringify(requests[0].messages);
  assert.match(first, /INVOICE_EVIDENCE/, 'the request\'s own file is in front of the agent');
  assert.match(first, /request_files/);
  assert.ok(!first.includes('QUOTE_EVIDENCE'), 'the old quote is outside recent context');
  assert.ok(!first.includes('FOREIGN_PAYSLIP_EVIDENCE'));
  const tools = requests.flatMap(request => request.messages.filter(m => m.role === 'tool').map(m => m.content)).join('\n');
  assert.match(tools, /quote-acme\.txt/);
  assert.match(tools, /QUOTE_EVIDENCE/);
  assert.match(tools, /not_found/);
  assert.ok(!tools.includes('FOREIGN_PAYSLIP_EVIDENCE'), 'a file from another conversation never reaches the agent');
  assert.equal(Number(app.sql('SELECT count(*) FROM relay_control_actions')), 0);
  app.pass('A real pooled Goose worker reads the request\'s file in its prompt, finds an older file outside recent context with the file tools, and is refused a file from another conversation');
  await writeFile(join(app.data, 'chat-files-model-requests.json'), JSON.stringify(requests, null, 2), { mode: 0o600 });
});
