// Opt-in: disposable local containers, synthetic secrets, no live identity/model requests.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdir, mkdtemp, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { once } from 'node:events';
import { get as httpGet } from 'node:http';
import { join, resolve } from 'node:path';
import { randomUUID } from 'node:crypto';
import { until } from './fixtures/polling.mjs';

test('deployable image runs separate API, migration and supervised worker containers', {
  timeout: 240000, skip: process.env.AYATI_CONTAINER_TEST !== '1',
}, async t => {
  const root = resolve(import.meta.dirname,'..');
  await mkdir(join(root,'.tmp'),{recursive:true});
  const data = await mkdtemp(join(root,'.tmp/container-deployment-'));
  const name = 'ayati-test-' + randomUUID().slice(0,8);
  const listener = createServer().listen(0,'127.0.0.1');
  await once(listener,'listening');
  const port = listener.address().port;
  await new Promise(resolve => listener.close(resolve));
  const secret = async (name,value) => {
    const path = join(data,name);
    // Synthetic files in a private directory; readable by the image's non-root UID.
    await writeFile(path,value,{mode:0o444}); return path;
  };
  const env = {
    AYATI_IMAGE: process.env.AYATI_CONTAINER_IMAGE ?? 'ayati:hosted-local',
    AYATI_PUBLIC_ORIGIN: 'https://workspace.example.test', AYATI_LOCAL_PORT: String(port),
    AYATI_WORKER_PROVIDERS: 'openai', AYATI_MODEL: 'fixture', WORKOS_CLIENT_ID:'client_fixture',
    AYATI_DATABASE_URL_FILE: await secret('database-url','postgresql://relay_app:fixture-runtime@db:5432/ayati'),
    AYATI_MIGRATION_DATABASE_URL_FILE: await secret('migration-url','postgresql://postgres:fixture-admin@db:5432/ayati'),
    AYATI_WORKOS_API_KEY_FILE: await secret('identity-key','fixture-unused-key'),
    AYATI_MODEL_API_KEY_FILE: await secret('model-key','$(touch /tmp/secret-executed)'),
  };
  const envFile = join(data,'.env');
  await writeFile(envFile,Object.entries(env).map(([k,v]) => `${k}=${v}`).join('\n'));
  const override = join(data,'rehearsal.yaml');
  await writeFile(override,`services:
  db:
    image: postgres:18.6@sha256:edb0c08dc33befd0262ab3b5cf1c4f3f8cfd8b09e88c05b45adee29d6761ddb6
    environment:
      POSTGRES_DB: ayati
      POSTGRES_PASSWORD: fixture-admin
    tmpfs: [/var/lib/postgresql]
`);
  const args = ['compose','--project-name',name,'--env-file',envFile,
    '-f',join(root,'deploy/compose.yaml'),'-f',override];
  const docker = (input,options = {}) => execFileSync('docker',input,{
    encoding:'utf8',timeout:90000,maxBuffer:2*1024*1024,stdio:['pipe','pipe','pipe'],...options,
  }).trim();
  const compose = (input,options) => docker([...args,...input],options);
  const sql = input => compose(['exec','-T','db','psql','-U','postgres','-d','ayati','-At','-v','ON_ERROR_STOP=1'],{input});
  const checks = [];
  const pass = name => { checks.push(name); console.log('PASS',name); };
  t.after(async () => {
    try { await writeFile(join(data,'containers.log'),compose(['logs','--no-color'])); } catch {}
    try { compose(['--profile','maintenance','down','--volumes','--remove-orphans']); }
    finally {
      await writeFile(join(data,'report.json'),JSON.stringify({passed:t.passed,checks},null,2));
      console.log('Retained container evidence:',data);
    }
  });
  compose(['config','--quiet']);
  compose(['up','-d','db']);
  await until(() => { try { compose(['exec','-T','db','pg_isready','-h','127.0.0.1','-U','postgres']); return true; } catch { return false; } },'Fixture database did not start',30000);
  sql("CREATE ROLE relay_app LOGIN PASSWORD 'fixture-runtime';");
  compose(['--profile','maintenance','run','--rm','migrate']);
  sql(`GRANT USAGE ON SCHEMA public TO relay_app;
    GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA public TO relay_app;
    GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA public TO relay_app;
    REVOKE UPDATE,DELETE ON relay_audit FROM relay_app;
    REVOKE INSERT,UPDATE,DELETE ON seaql_migrations FROM relay_app;`);
  assert.equal(sql('SELECT count(*) FROM seaql_migrations'),'11');
  assert.equal(sql("SELECT has_schema_privilege('relay_app','public','CREATE')"),'f');
  pass('Explicit migration container creates the schema; runtime role has no DDL/migration authority');
  compose(['up','-d','api','worker']);
  const healthy = async service => until(() => {
    const id = compose(['ps','-q',service]);
    return id && docker(['inspect','--format','{{.State.Health.Status}}',id]) === 'healthy';
  },service+' container did not become healthy',45000);
  await healthy('api'); await healthy('worker');
  assert.equal(compose(['exec','-T','api','id','-u']),'10001');
  assert.equal(compose(['exec','-T','worker','/opt/ayati/goose','--version']),'1.54.0');
  compose(['exec','-T','api','sh','-c','test -z "${OPENAI_API_KEY_FILE-}"']);
  compose(['exec','-T','worker','sh','-c','test -z "${WORKOS_API_KEY_FILE-}"; test ! -e /tmp/secret-executed']);
  assert.throws(() => compose(['run','--rm','--no-deps','-e','OPENAI_API_KEY=fixture-conflict','worker','help']), /Configure either OPENAI_API_KEY or OPENAI_API_KEY_FILE/);
  assert.throws(() => compose(['run','--rm','--no-deps','-e','OPENAI_API_KEY_FILE=/missing-secret','worker','help']), /Cannot read OPENAI_API_KEY_FILE/);
  const workerId = compose(['ps','-q','worker']);
  assert.equal(docker(['inspect','--format','{{.HostConfig.ReadonlyRootfs}}',workerId]),'true');
  pass('Same image runs as UID 10001 with read-only root, pinned Goose and separate role secrets');
  // Node fetch does not preserve a custom Host; use HTTP transport to emulate the TLS proxy.
  const get = (path,cookie='') => new Promise((resolve,reject) => {
    httpGet({host:'127.0.0.1',port,path,headers:{Host:'workspace.example.test',Cookie:cookie,Connection:'close'}},res => {
      res.resume(); res.on('end',()=>resolve({status:res.statusCode,headers:res.headers}));
    }).on('error',reject);
  });
  const login = await get('/auth/login');
  assert.equal(login.status,303);
  const cookie = login.headers['set-cookie'][0].split(';')[0];
  assert.match(login.headers['set-cookie'][0],/; Secure/);
  assert.equal(sql('SELECT count(*) FROM relay_login_flows'),'1');
  compose(['rm','-sf','api']);
  compose(['up','-d','api']); await healthy('api');
  assert.equal(sql('SELECT count(*) FROM relay_login_flows'),'1');
  assert.equal((await get('/auth/callback?error=access_denied',cookie)).status,303);
  assert.equal(sql('SELECT count(*) FROM relay_login_flows'),'0');
  assert.equal((await get('/index.html')).status,200);
  pass('Replacing the API container loses scratch only; shared login state and packaged website remain available');
  compose(['stop','-t','5','db']);
  assert.throws(() => compose(['exec','-T','api','/opt/ayati/ayati','api-health']));
  await until(() => {
    try { compose(['exec','-T','worker','/opt/ayati/ayati','worker-health']); return false; }
    catch { return true; }
  },'Worker did not report database loss',20000);
  assert.equal((await get('/health/live')).status,200);
  compose(['stop','-t','20','worker']);
  assert.equal(docker(['inspect','--format','{{.State.ExitCode}}',workerId]),'0');
  pass('Database loss fails API/worker readiness while liveness stays up; worker drains on SIGTERM');
});
