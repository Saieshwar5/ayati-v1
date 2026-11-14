import assert from 'node:assert/strict';
import { createHash, randomBytes } from 'node:crypto';
import { test } from 'node:test';
import {
  ApiError,
  EventStreamParser,
  base64url,
  conversationKind,
  conversationName,
  createClient,
  directAgent,
  filterConversations,
  makePkce,
  mergeMessage,
  readReturn,
  startUrl,
  type Conversation,
  type Directory,
  type Message,
  type StreamEvent,
} from '../src/index';

test('base64url matches Node and PKCE uses SHA-256 of the verifier', async () => {
  for (const size of [0, 1, 2, 3, 31, 32, 33]) {
    const bytes = randomBytes(size);
    assert.equal(base64url(bytes), bytes.toString('base64url'));
  }
  const pkce = await makePkce(
    count => randomBytes(count),
    async text => createHash('sha256').update(text).digest(),
  );
  assert.equal(pkce.verifier.length, 43);
  assert.equal(pkce.challenge.length, 43);
  assert.equal(pkce.challenge, createHash('sha256').update(pkce.verifier).digest('base64url'));
});

test('sign-in addresses carry the challenge and the return is read strictly', () => {
  const url = new URL(startUrl('https://cuckoo.example', 'c'.repeat(43), { google: true, email: ' a@b.c ' }));
  assert.equal(url.pathname, '/auth/mobile/start');
  assert.equal(url.searchParams.get('challenge'), 'c'.repeat(43));
  assert.equal(url.searchParams.get('method'), 'google');
  assert.equal(url.searchParams.get('email'), 'a@b.c');
  const code = 'abcdefghijklmnopqrstuvwxyz012345678';
  assert.deepEqual(readReturn(`https://cuckoo.example/auth/mobile/return?code=${code}`), { code });
  assert.deepEqual(readReturn(`cuckoo-dev://auth/return?code=${code}#`), { code });
  assert.deepEqual(readReturn('cuckoo-dev://auth/return?error=sign_in_failed'), { error: 'sign_in_failed' });
  assert.deepEqual(readReturn('cuckoo-dev://auth/return?code=<script>'), { error: 'missing_code' });
});

test('the event-stream parser handles split chunks, CRLF, comments and ids', () => {
  const events: StreamEvent[] = [];
  const parser = new EventStreamParser(event => events.push(event));
  parser.push(': hello\r\nevent: changes\r\nid: 7\r\ndata: {"cursor"');
  parser.push(':7}\r');
  parser.push('\n\r\nevent: alive\ndata: {}\n\n');
  parser.push('data: one\ndata: two\n\n');
  assert.deepEqual(events, [
    { event: 'changes', data: '{"cursor":7}', id: '7' },
    { event: 'alive', data: '{}', id: '7' },
    { event: 'message', data: 'one\ntwo', id: '7' },
  ]);
  assert.equal(parser.lastId, '7');
});

const directory: Directory = {
  role: 'Member',
  users: [
    { id: 'me', name: 'Me' },
    { id: 'asha', name: 'Asha' },
  ],
  agents: [{ id: 'nova', name: 'Nova' }],
};
const chat = (id: string, kind: Conversation['kind'], users: string[], agents: string[] = []): Conversation => ({
  id,
  name: `#${id}`,
  kind,
  description: '',
  users,
  agents,
});

test('conversations are named and grouped for the viewer', () => {
  const withAgent = chat('a', 'direct', ['me'], ['nova']);
  const withPerson = chat('p', 'direct', ['me', 'asha']);
  const channel = chat('c', 'channel', ['me', 'asha'], ['nova']);
  assert.equal(conversationName(withAgent, directory, 'me'), 'Nova');
  assert.equal(conversationName(withPerson, directory, 'me'), 'Asha');
  assert.equal(conversationName(channel, directory, 'me'), '#c');
  assert.equal(conversationKind(withAgent, directory, 'me'), 'Agent');
  assert.equal(directAgent(withAgent, directory, 'me'), 'nova');
  assert.equal(directAgent(channel, directory, 'me'), undefined);
  const all = [withAgent, withPerson, channel, { ...chat('old', 'channel', ['me']), archived: true }];
  assert.deepEqual(filterConversations(all, 'all', directory, 'me', {}).map(c => c.id), ['a', 'p', 'c']);
  assert.deepEqual(filterConversations(all, 'agents', directory, 'me', {}).map(c => c.id), ['a']);
  assert.deepEqual(filterConversations(all, 'unread', directory, 'me', { p: 2 }).map(c => c.id), ['p']);
});

test('live messages replace older copies and keep time order', () => {
  const message = (id: string, at: number, revision = 0, content = id): Message => ({
    id, author: 'nova', name: 'Nova', kind: 'agent', content, state: 'sent', at, revision,
  });
  const history = [message('a', 1), message('c', 3)];
  assert.deepEqual(mergeMessage(history, message('b', 2)).map(m => m.id), ['a', 'b', 'c']);
  assert.equal(mergeMessage(history, message('c', 3, 1, 'new'))[1].content, 'new');
  const newer = [message('a', 1, 5, 'kept')];
  assert.equal(mergeMessage(newer, message('a', 1, 4, 'stale')), newer);
});

test('the client sends the bearer token and marker, never cookies, and reports 401', async () => {
  const seen: { url: string; init?: RequestInit }[] = [];
  let signedOut = false;
  const client = createClient({
    origin: 'https://cuckoo.example',
    token: () => 'token-value',
    onUnauthorized: () => { signedOut = true; },
    fetch: async (url, init) => {
      seen.push({ url, init });
      return url.endsWith('/devices')
        ? new Response(JSON.stringify({ error: 'Session ended or identity changed. Sign in again.' }), { status: 401 })
        : new Response(JSON.stringify({ chats: [] }), { status: 200 });
    },
  });
  await client.workspace('w 1').send('c', { id: 'x', content: 'hi', attachments: [] });
  const sent = seen[0];
  assert.equal(sent.url, 'https://cuckoo.example/api/identity/workspaces/w%201/chats/c/messages');
  assert.equal(sent.init?.method, 'POST');
  assert.equal(sent.init?.credentials, 'omit');
  assert.deepEqual(sent.init?.headers, {
    'X-Ayati-Request': 'identity',
    Authorization: 'Bearer token-value',
    'Content-Type': 'application/json',
  });
  await assert.rejects(client.devices(), (error: unknown) => error instanceof ApiError && error.status === 401 && error.denied);
  assert.equal(signedOut, true);
  assert.equal(
    client.workspace('w').liveUrl('12'),
    'https://cuckoo.example/api/identity/workspaces/w/message-events?compact=true&after=12',
  );
});

test('panels show in the web order, without hidden or empty ones, and chart numbers read as a table', async () => {
  const { shownPanels, chartTable, comparisonView, progressView } = await import('../src/panels');
  const panel = (id: string, sections = 1) => ({
    id, revision: 1, agent_name: 'Nova',
    active: { revision: 1, available: true, definition: { title: id, purpose: '', sections: Array.from({ length: sections }, () => ({ kind: 'text' as const, heading: 'h', body: 'b' })) } },
  });
  const page = { available: true, panels: [panel('a'), panel('b'), panel('c'), panel('empty', 0)], layout: { revision: 1, order: ['c', 'a'], wide: [], hidden: ['b'] } };
  assert.deepEqual(shownPanels(page).map(p => p.id), ['c', 'a']);
  const table = chartTable({ kind: 'chart', title: 'Sales', unit: '€', series: [
    { name: 'This week', points: [{ label: 'Mon', value: 10 }, { label: 'Tue', value: 12.5 }] },
    { name: 'Last week', points: [{ label: 'Tue', value: 9 }] },
  ] });
  assert.deepEqual(table.columns, ['', 'This week', 'Last week']);
  assert.deepEqual(table.rows[0], ['Mon', '10 €', '']);
  assert.equal(comparisonView({ label: 'vs last week', value: -3, favorable: 'decrease' }).sentiment, 'favorable');
  assert.equal(progressView({ id: 'p', label: 'Goal', actual: 50, target: 200 }).width, 25);
});

test('approval fields read as words and question drafts are checked before sending', async () => {
  const { fieldEntries, waitingLine, answerValues, draftProblem, answerLabels } = await import('../src/decisions');
  assert.deepEqual(fieldEntries({ invoice_id: 'INV-7', amount: 120.5, send_copy: true }), [
    ['invoice id', 'INV-7'], ['amount', '120.5'], ['send copy', 'Yes'],
  ]);
  assert.equal(waitingLine({ state: 'pending', may_decide: true }), 'Waiting for your decision');
  assert.equal(waitingLine({ state: 'pending', waiting_for: ['Ravi', 'Meera'] }), 'Waiting for approval from Ravi or Meera');
  assert.equal(waitingLine({ state: 'completed' }), 'Completed');

  const single = { version: 1, kind: 'single_choice', title: 'Which?', description: '', fallback: '', allow_other: true,
    options: [{ id: 'a', label: 'Monday' }, { id: 'b', label: 'Tuesday' }], fields: [] };
  assert.equal(draftProblem(single, { choices: [], fields: {} }), 'Choose an answer.');
  assert.equal(draftProblem(single, { choices: [], other: ' ', fields: {} }), 'Write your other answer.');
  assert.equal(draftProblem(single, { choices: ['a'], fields: {} }), undefined);
  assert.deepEqual(answerValues(single, { choices: ['a'], fields: {} }), { choices: ['a'] });

  const form = { ...single, kind: 'form', options: [], fields: [
    { id: 'qty', label: 'Quantity', kind: 'number', required: true, description: '', options: [] },
    { id: 'rush', label: 'Rush', kind: 'boolean', required: false, description: '', options: [] },
  ] };
  assert.equal(draftProblem(form, { choices: [], fields: {} }), 'Fill in Quantity.');
  assert.deepEqual(answerValues(form, { choices: [], fields: { qty: '3', rush: 'true' } }), { qty: 3, rush: true });
  assert.deepEqual(
    answerLabels({ id: 'q', key: 'k', revision: 1, state: 'answered', values: { qty: 3, rush: false }, specification: form,
      respondent: 'me', respondent_name: 'Me', can_respond: false, reply: 'r', chat: 'c' }).map(a => a.value),
    ['3', 'No'],
  );
});
