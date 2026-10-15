// End-to-end HTTP smoke with a scripted model and real binary/sandbox. No paid API calls.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

const binary = resolve('target/release/ayati');
const data = await mkdtemp(join(tmpdir(), 'ayati-smoke-'));
const requests = [];
const fixture = createServer(async (request, response) => {
  const chunks = [];
  for await (const chunk of request) chunks.push(chunk);
  const body = JSON.parse(Buffer.concat(chunks).toString());
  requests.push(body);
  const user = body.messages.findLast((message) => message.role === 'user').content;
  const last = body.messages.at(-1);
  response.writeHead(200, { 'content-type': 'text/event-stream' });
  const event = (delta, finish_reason = null) => `data: ${JSON.stringify({ choices: [{ delta, finish_reason }] })}\n\n`;
  response.write(event({ content: 'I’m checking your file. ' }));
  await delay(60);
  if (last.role === 'tool' || user.includes('What did')) {
    response.write(event({ content: 'The revised file is ready and verified.' }));
    response.end(event({}, 'stop') + 'data: [DONE]\n\n');
    return;
  }
  const path = user.match(/\/workspace\/[\w.-]+/)?.[0];
  const command = user.includes('long task')
    ? 'printf started > started.txt; sleep 30; printf late > late.txt'
    : `tr a-z A-Z < '${path}' > revised.txt; cat revised.txt`;
  const argumentsText = JSON.stringify({ command });
  const split = Math.floor(argumentsText.length / 2);
  response.write(event({ tool_calls: [{ index: 0, id: 'fixture_call', type: 'function',
    function: { name: 'shell', arguments: argumentsText.slice(0, split) } }] }));
  await delay(60);
  response.end(event({ tool_calls: [{ index: 0, function: { arguments: argumentsText.slice(split) } }] }, 'tool_calls') + 'data: [DONE]\n\n');
});
fixture.listen(0, '127.0.0.1');
await once(fixture, 'listening');

// Reserve an available port before launching the local daemon.
const reservation = createServer();
reservation.listen(0, '127.0.0.1');
await once(reservation, 'listening');
const port = reservation.address().port;
await new Promise((done) => reservation.close(done));
const base = `http://127.0.0.1:${port}`;
const environment = { ...process.env, AYATI_DATA_DIR: data, AYATI_PORT: String(port),
  AYATI_MODEL_BASE_URL: `http://127.0.0.1:${fixture.address().port}`,
  AYATI_MODEL: 'fixture-only', FIREWORKS_API_KEY: 'fixture-only-placeholder' };
let child;
let logs = '';
const start = () => {
  child = spawn(binary, ['serve'], { env: environment, stdio: ['ignore', 'pipe', 'pipe'] });
  child.stdout.on('data', (chunk) => { logs = (logs + chunk).slice(-16000); });
  child.stderr.on('data', (chunk) => { logs = (logs + chunk).slice(-16000); });
  child.on('error', (error) => { logs += error.message; });
};
async function eventually(check, label) {
  for (let attempt = 0; attempt < 150; attempt++) {
    if (child.exitCode !== null) throw new Error(`Daemon exited during ${label}: ${logs}`);
    try { if (await check()) return; } catch { /* Service may still be starting. */ }
    await delay(30);
  }
  throw new Error(`Timed out: ${label}. ${logs}`);
}
async function json(path, options) {
  const response = await fetch(base + path, options);
  assert.equal(response.ok, true, await response.clone().text());
  return response.json();
}
const post = (path, value = {}) => json(path, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(value) });
const state = () => json('/api/state');
let stream;
try {
  start();
  await eventually(async () => (await json('/api/health')).model_ready, 'startup');
  const html = await (await fetch(base)).text();
  assert.match(html, /Your assistant/);
  const asset = html.match(/src="([^"]+\.js)"/)[1];
  assert.equal((await fetch(base + asset)).ok, true);
  const rss = (await readFile(`/proc/${child.pid}/status`, 'utf8')).match(/^VmRSS:\s+(\d+) kB/m)?.[1];

  const eventAbort = new AbortController();
  const events = await fetch(base + '/api/events', { signal: eventAbort.signal });
  let eventText = '';
  stream = (async () => {
    try { for await (const chunk of events.body) eventText += Buffer.from(chunk).toString(); }
    catch { /* Intentional stream close during shutdown. */ }
  })();
  const form = new FormData();
  form.append('file', new Blob(['my original notes\n']), 'notes.txt');
  const upload = await json('/api/files', { method: 'POST', body: form });
  const task = await post('/api/messages', { content: `Please revise this file: ${upload.workspace_path}` });
  await eventually(async () => (await state()).tasks.find((t) => t.id === task.id)?.status === 'completed', 'file task');
  let saved = await state();
  const output = saved.artifacts.find((file) => file.name === 'revised.txt');
  assert.ok(output);
  assert.equal(await (await fetch(base + `/api/files/${output.id}`)).text(), 'MY ORIGINAL NOTES\n');
  assert.equal(await (await fetch(base + `/api/files/${upload.artifact.id}`)).text(), 'my original notes\n');
  assert.match(eventText, /"kind":"delta"/);
  assert.match(eventText, /"offset":0/);
  assert.equal(saved.messages.filter((message) => message.role === 'user').length, 1);
  assert.ok(saved.messages.some((message) => message.content.includes('ready and verified')));

  const followup = await post('/api/messages', { content: 'What did we just do?' });
  await eventually(async () => (await state()).tasks.find((t) => t.id === followup.id)?.status === 'completed', 'follow-up');
  assert.ok(requests.at(-1).messages.some((message) => message.content?.includes(upload.workspace_path)));
  const long = await post('/api/messages', { content: 'Please run a long task' });
  await eventually(async () => (await state()).tasks.find((t) => t.id === long.id)?.detail === 'Working in your workspace', 'shell start');
  await post(`/api/tasks/${long.id}/stop`);
  await eventually(async () => (await state()).tasks.find((t) => t.id === long.id)?.status === 'stopped', 'cancellation');
  await assert.rejects(readFile(join(data, 'workspace', 'late.txt')));

  const exit = once(child, 'exit');
  await post('/api/shutdown');
  await Promise.race([exit, delay(6000).then(() => { throw new Error('Shutdown hung with an open event stream'); })]);
  eventAbort.abort();
  await stream;
  assert.equal(child.exitCode, 0);
  start();
  await eventually(async () => (await json('/api/health')).status === 'ready', 'restart');
  saved = await state();
  assert.equal(saved.tasks.find((t) => t.id === task.id).status, 'completed');
  assert.equal(saved.tasks.find((t) => t.id === long.id).status, 'stopped');
  assert.equal(saved.messages.filter((message) => message.role === 'user').length, 3);
  assert.equal(await (await fetch(base + `/api/files/${output.id}`)).text(), 'MY ORIGINAL NOTES\n');
  assert.equal(requests.length, 4, 'Restart must not replay work');
  const restartedExit = once(child, 'exit');
  await post('/api/shutdown');
  await restartedExit;
  console.log(JSON.stringify({ passed: true, model: 'local scripted fixture', model_requests: requests.length,
    checks: ['static client', 'HTTP upload', 'streamed response', 'real shell', 'download', 'original preserved', 'follow-up', 'stop', 'SSE shutdown', 'restart persistence'],
    idle_rss_kb: Number(rss) }));
} finally {
  if (child && child.exitCode === null) child.kill('SIGKILL');
  fixture.closeAllConnections();
  await new Promise((done) => fixture.close(done));
  await rm(data, { recursive: true, force: true });
}
