import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { once } from 'node:events';
import { mkdir, mkdtemp, writeFile, symlink, readFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { join, resolve } from 'node:path';
import { randomUUID } from 'node:crypto';
import { fixtureProvider } from './goose_provider.mjs';
import { until } from './polling.mjs';
import { loadProfile, limitCommand } from './load_limits.mjs';
import { imageRuntime } from './image_runtime.mjs';

const root = resolve(import.meta.dirname, '../..');

async function freePort() {
  const listener = createServer().listen(0, '127.0.0.1');
  await once(listener, 'listening');
  const { port } = listener.address();
  await new Promise(resolve => listener.close(resolve));
  return port;
}
const binary = process.env.AYATI_TEST_BIN ?? join(root, 'target/debug/ayati');

export async function workerFixture(t, options = {}) {
  const limitsProfile = await loadProfile();
  const admin = new URL(process.env.RELAY_TEST_DATABASE_URL);
  assert.match(admin.pathname, /^\/relay_simulation_/,
    'Process tests require a dedicated local relay_simulation_* test database');
  assert.ok(['localhost', '127.0.0.1'].includes(admin.hostname));
  assert.equal(admin.searchParams.has('options'), false, 'Use a base test database URL');
  await mkdir(join(root, '.tmp'), { recursive: true });
  const data = await mkdtemp(join(root, '.tmp/independent-worker-'));
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
        timeout: 30000,
        stdio: ['pipe', 'pipe', 'pipe'],
      }).trim();
    } catch { throw new Error('Disposable worker fixture SQL failed'); }
  }
  async function sqlAsync(input, url = admin.href) {
    const connection=new URL(url);
    return new Promise((resolve,reject)=>{
      const child=spawn('psql',['-X','-At','-v','ON_ERROR_STOP=1'],{env:{...process.env,
        PGDATABASE:decodeURIComponent(connection.pathname.slice(1)),PGHOST:connection.searchParams.get('host')??connection.hostname,
        PGPORT:connection.searchParams.get('port')??(connection.port||'5432'),PGUSER:decodeURIComponent(connection.username),
        PGPASSWORD:decodeURIComponent(connection.password),PGOPTIONS:connection.searchParams.get('options')??'',
      },stdio:['pipe','pipe','pipe']});
      let value='';child.stdout.on('data',chunk=>{value+=chunk;if(value.length>2097152)child.kill();});
      child.stderr.resume();child.on('error',()=>reject(new Error('Disposable async SQL could not start')));
      child.stdin.on('error',()=>reject(new Error('Disposable async SQL input failed')));
      child.on('close',code=>code===0?resolve(value.trim()):reject(new Error('Disposable async SQL failed')));child.stdin.end(input);
    });
  }
  sql(`CREATE SCHEMA ${schema}`);
  const containers = process.env.AYATI_TEST_IMAGE_CONFIG ? imageRuntime(data, limitsProfile, sql) : undefined;
  const scoped = new URL(admin);
  scoped.searchParams.set('options', `-c search_path=${schema}`);
  const runtime = new URL(scoped);
  runtime.username = 'relay_app';
  const provider = options.provider ?? await fixtureProvider({ resumableHold: options.resumableHold ?? false });
  t.after(() => provider.close());
  const env = {
    PATH: process.env.PATH, LANG: 'C.UTF-8', RUST_LOG: 'ayati=info',
    // Never read a developer's .env or .env.executor from a parent directory.
    AYATI_DOTENV: '0',
    AYATI_MODEL: options.model ?? 'fixture',
    AYATI_GOOSE_PROVIDER: 'openai', AYATI_GOOSE_BIN: join(root, '.cache/goose/v1.54.0/goose'),
    AYATI_PREPARED_GOOSE_BIN: process.env.AYATI_TEST_PREPARED_GOOSE_BIN ?? join(root, '.cache/goose/v1.54.0-ayati.1/goose'),
    // Unset runs the simulation's scripted replies; 'pooled' uses real executors.
    ...(options.mode ? { AYATI_CHAT_EXECUTION: options.mode } : {}),
    AYATI_ASSIGNMENT_RUN_SECONDS: String(options.runSeconds ?? 180),
    AYATI_DATA_DIR: join(data, 'migration'),
    AYATI_TRACE_START:options.traceStart?'1':'0',
  };
  let control;
  // Settings only the controller (TLS listener) or only executors (trusted CA) may receive.
  let controllerControl = {}, executorControl = {};
  if (options.control) {
    const listener = createServer().listen(0, '127.0.0.1');
    await once(listener, 'listening');
    const controlPort = listener.address().port;
    await new Promise(resolve => listener.close(resolve));
    const { randomBytes } = await import('node:crypto');
    const bootstrap = randomBytes(32).toString('base64url');
    const keyFile = join(data, 'worker-control.key');
    await writeFile(keyFile, bootstrap + '\n', { mode: 0o600 });
    control = { url: `http://127.0.0.1:${controlPort}`, bootstrap, keyFile };
    if (options.tls) {
      // A private network link: TLS signed by a private authority executors trust.
      const certs = join(data, 'worker-control-tls');
      execFileSync(join(root, 'tools/worker-control-certs.sh'), [certs, 'localhost', '127.0.0.1'], { stdio: 'ignore' });
      control = { ...control, url: `https://localhost:${controlPort}`, port: controlPort, ca: join(certs, 'ca.pem'), certs };
      controllerControl = { AYATI_WORKER_CONTROL_LISTEN: `127.0.0.1:${controlPort}`,
        AYATI_WORKER_CONTROL_TLS_CERT: join(certs, 'server.pem'), AYATI_WORKER_CONTROL_TLS_KEY: join(certs, 'server.key') };
      executorControl = { AYATI_WORKER_CONTROL_CA: control.ca };
    }
    env.AYATI_WORKER_CONTROL_URL = control.url;
    env.AYATI_WORKER_CONTROL_KEY_FILE = keyFile;
    env.AYATI_ASSIGNMENT_LEASE_SECONDS = String(options.leaseSeconds??6);
    env.AYATI_EXECUTOR_CAPACITY = String(options.capacity ?? 2);
    env.AYATI_EXECUTOR_WORKSPACE_LIMIT = String(options.workspaceLimit ?? 2);
    env.AYATI_EXECUTOR_MODEL_LIMIT = String(options.modelLimit ?? 2);
    env.AYATI_EXECUTOR_CHAT_RESERVE = String(options.chatReserve ?? 0);
  }
  if (containers) containers.migrate(scoped.href,env,schema);
  else execFileSync(binary, ['workspace-db', 'migrate'], {
    cwd: data, env: { ...env, RELAY_DATABASE_URL: scoped.href }, stdio: 'pipe', timeout: 30000,
  });
  sql(`GRANT USAGE ON SCHEMA ${schema} TO relay_app;
    GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA ${schema} TO relay_app;
    GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA ${schema} TO relay_app;
    REVOKE UPDATE,DELETE ON ${schema}.relay_audit FROM relay_app;
    REVOKE INSERT,UPDATE,DELETE ON ${schema}.seaql_migrations FROM relay_app;`);
  const port = await freePort();
  const base = `http://127.0.0.1:${port}`;
  const children = [];
  function launch(args, extra, input) {
    const childEnv = Object.fromEntries(Object.entries({ ...env, ...extra }).filter(([, value]) => value !== undefined));
    if (containers) { const record=containers.launch(args,childEnv,input);children.push(record);return record; }
    if (limitsProfile) for (const name of ['DBUS_SESSION_BUS_ADDRESS', 'XDG_RUNTIME_DIR']) {
      if (process.env[name]) childEnv[name] = process.env[name];
    }
    const command = options.cpuList ? 'taskset' : binary;
    const arguments_ = options.cpuList ? ['--cpu-list', options.cpuList, binary, ...args] : args;
    const role = ['simulate', 'identity'].includes(args[0]) ? 'controller' : 'executor';
    const limited = limitCommand(limitsProfile, role, command, arguments_);
    const child = spawn(limited.command, limited.args, { cwd: data, env: childEnv, stdio: [input ? 'pipe' : 'ignore', 'pipe', 'pipe'] });
    if (input) child.stdin.end(JSON.stringify(input));
    const record = { child, logs: '', resource_scope: limited.unit };
    child.stdout.on('data', bytes => { record.logs += bytes; });
    child.stderr.on('data', bytes => { record.logs += bytes; });
    children.push(record);
    return record;
  }
  async function stop(record, signal = 'SIGTERM') {
    if (containers && record) return containers.stop(record,signal);
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
      ...controllerControl, ...options.apiEnv,
    });
    await until(async () => {
      assert.equal(api.child.exitCode, null, api.logs);
      try { return (await fetch(base + '/api/identity/simulation', { headers: { Connection: 'close' } })).ok; }
      catch { return false; }
    }, 'API did not start');
  }
  // Each controller checks the browser Origin against its own origin.
  const requestAt = at => async (path, cookie, body, expected = 200, method) => {
    const response = await fetch(at + path, {
      method: method ?? (body === undefined ? 'GET' : 'POST'),
      headers: { Cookie: cookie ?? '', Origin: at, 'X-Ayati-Request': 'identity',
        'Content-Type': 'application/json', Connection: 'close' },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const text = await response.text();
    assert.equal(response.status, expected, text);
    const value = text ? (response.headers.get('content-type')?.includes('application/json') ? JSON.parse(text) : text) : null;
    return { value, cookie: response.headers.get('set-cookie')?.split(';')[0] };
  };
  const request = requestAt(base);
  async function login(number) {
    return (await request(`/api/identity/simulation/employees/${number}/login`, '', {})).cookie;
  }
  const records = [];
  t.after(async () => {
    provider.setHeld?.(false);
    provider.setOutputHeld?.(false);
    const cleanupErrors = [];
    for (const child of [...children].reverse()) {
      try { await stop(child); }
      catch (error) { cleanupErrors.push(error); }
    }
    containers?.finish();
    await writeFile(join(data, 'report.json'), JSON.stringify({
      passed: t.passed, schema, checks: records, cleanup_errors: cleanupErrors.length,
      processes: children.map(c => ({ pid: c.child.pid, code: c.child.exitCode, signal: c.child.signalCode, logs: c.logs.slice(-65536) })),
    }, null, 2));
    for (const [i, child] of children.entries()) await writeFile(join(data, `process-${i}.log`), child.logs);
    console.log('Retained worker evidence:', data);
    if (cleanupErrors.length) throw new AggregateError(cleanupErrors, 'Worker fixture cleanup failed');
  });
  await startApi();
  const info = (await request('/api/identity/simulation')).value;
  const cookie = await login(0);
  const path = '/api/identity/workspaces/' + info.workspace;
  const profile = { id: randomUUID(), name: 'Nova', role: 'Answer bounded workspace questions.',
    model: options.model ?? 'fixture', scope: 'workspace', revision: 0, settings: { provider: 'openai' } };
  await request(path + '/agent-profiles', cookie, profile);
  const conversation = (await request(path + '/chats', cookie, {
    id: randomUUID(), name: 'Worker proof', users: [], agents: [profile.id], kind: 'direct',
  })).value.id;
  async function send(content, chat = conversation, actor = cookie, agent = profile.id) {
    const id = randomUUID();
    await request(`${path}/chats/${chat}/messages`, actor, { id, content, agent });
    return JSON.parse(sql(`SELECT to_json(j.payload)::text FROM relay_chat_jobs j JOIN relay_chat_messages r ON r.id=j.reply WHERE r.chat='${chat}' AND r.sequence>(SELECT sequence FROM relay_chat_messages WHERE id='${id}') ORDER BY r.sequence LIMIT 1`, scoped.href));
  }
  async function history(chat = conversation, actor = cookie) {
    return (await request(`${path}/chats/${chat}/messages`, actor)).value.messages;
  }
  async function done(job, state = 'complete') {
    return until(async () => (await history(job.chat)).find(m => m.id === job.reply && m.state === state),
      'Reply did not reach ' + state, 35000);
  }
  return { data, base, path, cookie, profile, conversation, provider, request, login, send, history, done, control, containers,
    startBridge(directory) {
      const record = containers.bridge(directory, control, provider);
      children.push(record);
      return record;
    },
    /** A further controller replica on the same database, with its own
     * website and worker-control ports, as a second cloud task would run. */
    async startController(extra = {}) {
      assert.ok(options.control && !options.tls, 'Replicas use loopback worker control');
      const dir = join(data, 'controller-' + randomUUID());
      await mkdir(dir, { mode: 0o700 });
      // Reuse the seeded company: its roster prevents seeding it a second time.
      await writeFile(join(dir, 'company.json'), await readFile(join(data, 'api', 'company.json')), { mode: 0o600 });
      await writeFile(join(dir, 'SIMULATION'), new URL(runtime.href).pathname, { mode: 0o600 });
      const [webPort, controlPort] = [await freePort(), await freePort()];
      const controlUrl = `http://127.0.0.1:${controlPort}`, at = `http://127.0.0.1:${webPort}`;
      const record = launch(['simulate', dir], { RELAY_SIMULATION_DATABASE_URL: runtime.href, AYATI_PORT: String(webPort),
        AYATI_WORKER_CONTROL_URL: controlUrl, ...options.apiEnv, ...extra });
      await until(async () => {
        assert.equal(record.child.exitCode, null, record.logs);
        try { return (await fetch(at + '/api/identity/simulation', { headers: { Connection: 'close' } })).ok; }
        catch { return false; }
      }, 'Controller replica did not start');
      return { record, base: at, controlUrl, request: requestAt(at) };
    },
    startHosted: extra => launch(['identity'], { RELAY_DATABASE_URL: runtime.href,
      WORKOS_CLIENT_ID: 'client_fixture', WORKOS_API_KEY: 'fixture-unused-key',
      AYATI_DATA_DIR: join(data, 'hosted-' + randomUUID()), ...extra }),
    sql: input => sql(input, scoped.href), startApi, stopApi: () => stop(api), stop,
    sqlAsync:input=>sqlAsync(input,scoped.href),
    apiProcess:()=>api,
    processes:()=>[...children],
    async startExecutor(extra = {}) {
      const workerData = join(data, 'executor-' + randomUUID());
      await mkdir(workerData);
      return launch(['executor'], { AYATI_DATA_DIR: workerData, RELAY_DATABASE_URL: undefined,
        RELAY_SIMULATION_DATABASE_URL: undefined, OPENAI_API_KEY: provider.key ?? 'fixture-not-a-secret',
        OPENAI_HOST: provider.url, AYATI_EXECUTOR_SLOTS: '2', ...executorControl, ...extra });
    },
    pass: name => { records.push({ name, passed: true }); console.log('PASS', name); },
    // Only synthetic IDs obtained from this schema are interpolated into fixture SQL.
    expire: job => sql(`UPDATE relay_chat_jobs SET lease_until=0 WHERE reply='${job.reply}' AND state='running'`, scoped.href),
    attempts: job => Number(sql(`SELECT attempts FROM relay_chat_jobs WHERE reply='${job.reply}'`, scoped.href)),
    async scratchFiles() {
      const { readdir } = await import('node:fs/promises');
      const files = await readdir(data, { recursive: true });
      return files.filter(file => file.endsWith('chat.sqlite'));
    },
    async executorIdentity(worker) {
      const clean=worker.logs.replace(/\u001b\[[0-9;]*m/g,'');
      const id=clean.match(/executor="?([a-f0-9-]{36})/)?.[1];
      assert.ok(id,'Executor has not published its durable identity');
      return JSON.parse(await sqlAsync(`SELECT row_to_json(e) FROM (SELECT id,incarnation FROM relay_executors WHERE id='${id}') e`,scoped.href));
    },
    async gooseChildren(worker) {
      if (containers) return containers.children(worker);
      try {
        const pids = execFileSync('ps', ['--ppid', String(worker.child.pid), '-o', 'pid='], { encoding: 'utf8' });
        return pids.trim().split(/\s+/).filter(Boolean).map(Number);
      } catch (error) {
        if (error.status === 1 && !error.stdout?.trim()) return [];
        throw error;
      }
    },
    async assignmentProcessIds(claims) {
      assert.ok(claims.every(id=>/^[a-f0-9-]{36}$/.test(id)));
      const values=JSON.parse(await sqlAsync(`SELECT COALESCE(json_agg(json_build_object('executor',executor,'pid',pid)),'[]') FROM relay_executor_slots WHERE assignment IN ('${claims.join("','")}')`,scoped.href));
      return containers?containers.hostPids(values):values.map(row=>row.pid);
    },
    async assertChildrenStopped(pids, timeout = 5000) {
      for (const pid of pids) {
        await until(async () => {
          try { return (await readFile(`/proc/${pid}/stat`, 'utf8')).split(') ')[1].startsWith('Z'); }
          catch(error) {if(error.code==='ENOENT'||error.code==='ESRCH')return true;throw error;}
        }, 'Goose survived its worker process: ' + pid, timeout);
      }
    },
  };
}
