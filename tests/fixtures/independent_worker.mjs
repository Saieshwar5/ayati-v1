import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { once } from 'node:events';
import { mkdir, mkdtemp, writeFile, symlink, readFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { join, resolve } from 'node:path';
import { randomUUID } from 'node:crypto';
import { fixtureProvider } from './goose_provider.mjs';
import { until } from './goose_app.mjs';

const root = resolve(import.meta.dirname, '../..');
const binary = process.env.AYATI_TEST_BIN ?? join(root, 'target/debug/ayati');

export async function workerFixture(t) {
  const admin = new URL(process.env.RELAY_TEST_DATABASE_URL);
  assert.match(admin.pathname, /^\/relay_simulation_/,
    'Process tests require a dedicated local relay_simulation_* test database');
  assert.ok(['localhost', '127.0.0.1'].includes(admin.hostname));
  assert.equal(admin.searchParams.has('options'), false, 'Use a base test database URL');
  await mkdir(join(root, '.tmp'), { recursive: true });
  const data = await mkdtemp(join(root, '.tmp/independent-worker-'));
  await writeFile(join(data, '.env'), ''); // Prevent dotenv from finding a real parent .env.
  await symlink(join(root, 'web'), join(data, 'web'));
  const schema = 'relay_worker_' + randomUUID().replaceAll('-', '');
  function sql(input, url = admin.href) {
    // Keep connection credentials out of argv and exception messages.
    const connection = new URL(url);
    try {
      return execFileSync('psql', ['-X', '-At', '-v', 'ON_ERROR_STOP=1'], {
        env: { ...process.env,
          PGDATABASE: decodeURIComponent(connection.pathname.slice(1)),
          PGHOST: connection.searchParams.get('host') ?? connection.hostname,
          PGPORT: connection.searchParams.get('port') ?? (connection.port || '5432'),
          PGUSER: decodeURIComponent(connection.username),
          PGPASSWORD: decodeURIComponent(connection.password),
          PGOPTIONS: connection.searchParams.get('options') ?? '',
        }, input, encoding: 'utf8',
        stdio: ['pipe', 'pipe', 'pipe'],
      }).trim();
    } catch { throw new Error('Disposable worker fixture SQL failed'); }
  }
  sql(`CREATE SCHEMA ${schema}`);
  const scoped = new URL(admin);
  scoped.searchParams.set('options', `-c search_path=${schema}`);
  const runtime = new URL(scoped);
  runtime.username = 'relay_app';
  const provider = await fixtureProvider();
  const env = {
    PATH: process.env.PATH, LANG: 'C.UTF-8', RUST_LOG: 'ayati=info',
    AYATI_RUNTIME: 'goose', AYATI_EXECUTION: 'host', AYATI_MODEL: 'fixture',
    AYATI_GOOSE_PROVIDER: 'openai', AYATI_GOOSE_BIN: join(root, '.cache/goose/v1.54.0/goose'),
    AYATI_CHAT_EXECUTION: 'external', AYATI_WORKER_CONCURRENCY: '1',
    AYATI_DATA_DIR: join(data, 'migration'),
  };
  execFileSync(binary, ['workspace-db', 'migrate'], {
    cwd: data, env: { ...env, RELAY_DATABASE_URL: scoped.href }, stdio: 'pipe',
  });
  sql(`GRANT USAGE ON SCHEMA ${schema} TO relay_app;
    GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA ${schema} TO relay_app;
    GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA ${schema} TO relay_app;
    REVOKE UPDATE,DELETE ON ${schema}.relay_audit FROM relay_app;
    REVOKE INSERT,UPDATE,DELETE ON ${schema}.seaql_migrations FROM relay_app;`);
  const listener = createServer().listen(0, '127.0.0.1');
  await once(listener, 'listening');
  const port = listener.address().port;
  await new Promise(resolve => listener.close(resolve));
  const base = `http://127.0.0.1:${port}`;
  const children = [];
  function launch(args, extra) {
    const child = spawn(binary, args, { cwd: data, env: { ...env, ...extra }, stdio: ['ignore', 'pipe', 'pipe'] });
    const record = { child, logs: '' };
    child.stdout.on('data', bytes => { record.logs += bytes; });
    child.stderr.on('data', bytes => { record.logs += bytes; });
    children.push(record);
    return record;
  }
  async function stop(record, signal = 'SIGTERM') {
    if (!record || record.child.exitCode !== null || record.child.signalCode !== null) return;
    const exited = once(record.child, 'exit');
    record.child.kill(signal);
    let timer;
    try {
      await Promise.race([exited, new Promise((_, reject) => {
        timer = setTimeout(() => { record.child.kill('SIGKILL'); reject(new Error('Process did not stop: ' + record.logs)); }, 15000);
      })]);
    } finally { clearTimeout(timer); }
    if (signal === 'SIGTERM') assert.equal(record.child.exitCode, 0, record.logs);
  }
  let api;
  async function startApi() {
    api = launch(['simulate', join(data, 'api')], {
      RELAY_SIMULATION_DATABASE_URL: runtime.href, AYATI_PORT: String(port),
    });
    await until(async () => {
      assert.equal(api.child.exitCode, null, api.logs);
      try { return (await fetch(base + '/api/identity/simulation', { headers: { Connection: 'close' } })).ok; }
      catch { return false; }
    }, 'API did not start');
  }
  async function startWorker() {
    const workerData = join(data, 'worker-' + randomUUID());
    await mkdir(workerData);
    const worker = launch(['worker'], {
      RELAY_DATABASE_URL: runtime.href,
      AYATI_DATA_DIR: workerData,
      AYATI_WORKER_HEALTH_FILE: join(workerData, 'worker.health'),
      OPENAI_API_KEY: 'fixture-not-a-secret', OPENAI_HOST: provider.url,
      // Deliberately inherited local capabilities must all be disabled for shared work.
      AYATI_EXECUTION: 'vm', AYATI_VM_MANIFEST: '/missing/private/vm.json',
      AYATI_BROWSER: '1', AYATI_MEMORY: '1', AYATI_PROOF_TOOL: '1',
    });
    worker.healthFile = join(workerData, 'worker.health');
    await until(() => {
      assert.equal(worker.child.exitCode, null, worker.logs);
      try {
        execFileSync(binary, ['worker-health'], { cwd: data, env: { ...env, AYATI_WORKER_HEALTH_FILE: worker.healthFile }, stdio: 'pipe' });
        return true;
      } catch { return false; }
    }, 'Worker did not start');
    return worker;
  }
  async function request(path, cookie, body, expected = 200, method) {
    const response = await fetch(base + path, {
      method: method ?? (body === undefined ? 'GET' : 'POST'),
      headers: { Cookie: cookie ?? '', Origin: base, 'X-Ayati-Request': 'identity',
        'Content-Type': 'application/json', Connection: 'close' },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const text = await response.text();
    assert.equal(response.status, expected, text);
    return { value: text ? JSON.parse(text) : null, cookie: response.headers.get('set-cookie')?.split(';')[0] };
  }
  async function login(number) {
    return (await request(`/api/identity/simulation/employees/${number}/login`, '', {})).cookie;
  }
  const records = [];
  t.after(async () => {
    for (const child of [...children].reverse()) await stop(child);
    provider.close();
    await writeFile(join(data, 'report.json'), JSON.stringify({
      passed: t.passed, schema, checks: records, processes: children.map(c => ({ pid: c.child.pid, code: c.child.exitCode, signal: c.child.signalCode })),
    }, null, 2));
    for (const [i, child] of children.entries()) await writeFile(join(data, `process-${i}.log`), child.logs);
    console.log('Retained worker evidence:', data);
  });
  await startApi();
  const info = (await request('/api/identity/simulation')).value;
  const cookie = await login(0);
  const path = '/api/identity/workspaces/' + info.workspace;
  const profile = { id: randomUUID(), name: 'Nova', role: 'Answer bounded workspace questions.',
    model: 'fixture', scope: 'workspace', revision: 0, settings: { provider: 'openai' } };
  await request(path + '/agent-profiles', cookie, profile);
  const conversation = (await request(path + '/chats', cookie, {
    id: randomUUID(), name: 'Worker proof', users: [], agents: [profile.id], kind: 'direct',
  })).value.id;
  async function send(content, chat = conversation, actor = cookie) {
    const id = randomUUID();
    await request(`${path}/chats/${chat}/messages`, actor, { id, content, agent: profile.id });
    return JSON.parse(sql(`SELECT to_json(j.payload)::text FROM relay_chat_jobs j JOIN relay_chat_messages r ON r.id=j.reply WHERE r.chat='${chat}' AND r.sequence>(SELECT sequence FROM relay_chat_messages WHERE id='${id}') ORDER BY r.sequence LIMIT 1`, scoped.href));
  }
  async function history(chat = conversation, actor = cookie) {
    return (await request(`${path}/chats/${chat}/messages`, actor)).value.messages;
  }
  async function done(job, state = 'complete') {
    return until(async () => (await history(job.chat)).find(m => m.id === job.reply && m.state === state),
      'Reply did not reach ' + state, 35000);
  }
  return { data, base, path, cookie, profile, conversation, provider, request, login, send, history, done,
    startHosted: extra => launch(['identity'], { RELAY_DATABASE_URL: runtime.href,
      WORKOS_CLIENT_ID: 'client_fixture', WORKOS_API_KEY: 'fixture-unused-key',
      AYATI_DATA_DIR: join(data, 'hosted-' + randomUUID()), ...extra }),
    sql: input => sql(input, scoped.href), startApi, stopApi: () => stop(api), startWorker, stop,
    pass: name => { records.push({ name, passed: true }); console.log('PASS', name); },
    // Only synthetic IDs obtained from this schema are interpolated into fixture SQL.
    expire: job => sql(`UPDATE relay_chat_jobs SET lease_until=0 WHERE reply='${job.reply}' AND state='running'`, scoped.href),
    attempts: job => Number(sql(`SELECT attempts FROM relay_chat_jobs WHERE reply='${job.reply}'`, scoped.href)),
    async scratchFiles() {
      const { readdir } = await import('node:fs/promises');
      const files = await readdir(data, { recursive: true });
      return files.filter(file => file.endsWith('chat.sqlite'));
    },
    async gooseChildren(worker) {
      const pids = execFileSync('ps', ['--ppid', String(worker.child.pid), '-o', 'pid='], { encoding: 'utf8' });
      return pids.trim().split(/\s+/).filter(Boolean).map(Number);
    },
    async assertChildrenStopped(pids) {
      for (const pid of pids) {
        await until(async () => {
          try { return (await readFile(`/proc/${pid}/stat`, 'utf8')).split(') ')[1].startsWith('Z'); }
          catch { return true; }
        }, 'Goose survived its worker process: ' + pid, 5000);
      }
    },
  };
}
