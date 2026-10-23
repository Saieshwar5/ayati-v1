import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { until } from './goose_app.mjs';
import { traces } from './load_metrics.mjs';

export async function ready(app, count = 4) {
  await until(async () => Number(await app.sqlAsync("SELECT count(*) FROM relay_executor_slots s JOIN relay_executors e ON e.id=s.executor WHERE s.state='ready' AND e.state='serving'")) >= count,
    'Compatible prepared reserve did not recover', 45000);
}
export async function actors(app, count, tier) {
  const result = [];
  for (let i = 0; i < count; i++) {
    const profile = { ...app.profile, id: randomUUID(), name: `${tier} load ${i}`, settings: { ...app.profile.settings, memory: tier === 'large' } };
    await app.request(app.path + '/agent-profiles', app.cookie, profile);
    if (tier === 'large') await app.request(`${app.path}/agent-profiles/${profile.id}/knowledge`, app.cookie, {
      id: randomUUID(), revision: 0, title: 'Approved load reference', content: `KNOWLEDGE_${profile.id} ${'approved evidence '.repeat(300)}`,
      chat: null, source_message: null, forgotten: false,
    });
    result.push(profile);
  }
  return result;
}
export async function requestCase(app, profile, tier) {
  const chat = (await app.request(app.path + '/chats', app.cookie, { id: randomUUID(), name: 'Isolated load conversation', kind: 'group', users: [], agents: [profile.id] })).value.id;
  if (tier === 'large') for (let i = 0; i < 4; i++) await app.request(`${app.path}/chats/${chat}/messages`, app.cookie, {
    id: randomUUID(), content: `HISTORY_${profile.id} ${'scoped history '.repeat(80)}`,
  });
  const id = randomUUID(), marker = `LOAD_${id}`;
  return { id, marker, chat, agent: profile.id, tier, content: marker + ' ' + 'x'.repeat(tier === 'large' ? 8000 : 256) };
}
export async function send(app, item) {
  item.sent_ms = Date.now();
  await app.request(`${app.path}/chats/${item.chat}/messages`, item.cookie??app.cookie, { id: item.id, content: item.content, agent: item.agent });
  item.ack_ms = Date.now();
}
export async function jobs(app, items) {
  // Only fixture-created UUIDs enter SQL. One asynchronous read per wave keeps
  // the model HTTP server responsive and bounds observer traffic.
  const chats = items.map(item => `'${item.chat}'`).join(',');
  return JSON.parse(await app.sqlAsync(`SELECT COALESCE(json_agg(row_to_json(r)),'[]') FROM (
    SELECT j.reply,j.state,j.attempts,j.claim,j.created_ms AS accepted_ms,j.finished_ms,
      j.payload->>'chat' AS chat,a.created_ms AS reserved_ms,a.context_ready_ms,a.first_output_ms,m.content,m.state AS message_state,m.progress
    FROM relay_chat_jobs j LEFT JOIN relay_worker_assignments a ON a.id=j.claim
    JOIN relay_chat_messages m ON m.id=j.reply WHERE j.payload->>'chat' IN (${chats})
  ) r`));
}
export async function complete(app, items, timeout = 60000) {
  return until(async () => {
    const values = await jobs(app, items);
    return values.length === items.length && values.every(v => v.state === 'finished') && values;
  }, 'Load wave did not finish', timeout);
}
export function samples(app, items, rows) {
  const trace = traces(app);
  return items.map(item => {
    const row = rows.find(row => row.chat === item.chat);
    assert.equal(row.message_state, 'complete', `Load reply failed: ${row.reply}`);
    assert.equal(row.attempts, 1, 'Healthy load cannot replay model inference');
    const matches = app.provider.requests.map((input, index) => ({ input, index })).filter(({ input }) => JSON.stringify(input.messages).includes(item.marker));
    assert.equal(matches.length, 1, 'Each accepted request must have one provider invocation');
    const { input, index } = matches[0];
    const markers = [...new Set(JSON.stringify(input.messages).match(/LOAD_[a-f0-9-]{36}/g))];
    assert.deepEqual(markers, [item.marker], 'Conversation isolation failed under load');
    const knowledge = [...new Set(JSON.stringify(input.messages).match(/KNOWLEDGE_[a-f0-9-]{36}/g) ?? [])];
    assert.deepEqual(knowledge, item.tier === 'large' ? [`KNOWLEDGE_${item.agent}`] : [], 'Knowledge escaped its agent scope');
    const timing = app.provider.timings.filter(t => t.index === index);
    const startTrace = trace.get(row.claim);
    assert.ok(startTrace?.controller && startTrace?.executor, 'Missing correlated start trace');
    return { ...row, sent_ms: item.sent_ms, ack_ms: item.ack_ms, trace: startTrace,
      provider_request_ms: timing.find(t => t.phase === 'request').time_ms,
      provider_first_token_ms: timing.find(t => t.phase === 'first_token').time_ms,
      prompt_bytes: Buffer.byteLength(JSON.stringify(input.messages)), content: undefined, progress: undefined };
  });
}
