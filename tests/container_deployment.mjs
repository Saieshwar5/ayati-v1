// Opt-in: disposable local containers, synthetic secrets, no live identity/model requests.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { get as httpGet } from 'node:http';
import { composeStack } from './fixtures/compose_stack.mjs';
import { until } from './fixtures/polling.mjs';

test('deployable image runs separate API, migration and pooled executor containers', {
  timeout: 240000, skip: process.env.AYATI_CONTAINER_TEST !== '1',
}, async t => {
  const stack = await composeStack(t, { prefix: 'container-deployment' });
  const { port, compose, docker, sql, pass } = stack;
  await stack.migrate();
  pass('Explicit migration container creates the schema; runtime role has no DDL/migration authority');
  compose(['up','-d','api','executor']);
  const healthy = stack.healthy;
  // Prepared Goose sessions make no model request before reporting ready.
  const ready = () => until(() => Number(sql("SELECT count(*) FROM relay_executor_slots WHERE state='ready'")) > 0,
    'Executor did not report prepared capacity',90000);
  await healthy('api'); await ready(); await healthy('executor');
  // The executor reaches worker control over TLS but has no route to the database.
  assert.throws(() => compose(['exec','-T','executor','/opt/ayati/bridge/bin/python','-c',
    "import socket; socket.create_connection(('db',5432),2)"]));
  compose(['up','-d','--no-recreate','--scale','executor=2','executor']);
  await until(() => Number(sql("SELECT count(*) FROM relay_executor_slots s JOIN relay_executors e ON e.id=s.executor AND e.incarnation=s.incarnation WHERE s.state='ready' AND e.state='serving'")) >= 4,
    'A second executor did not add ready slots',90000);
  compose(['up','-d','--no-recreate','--scale','executor=1','executor']);
  pass('Executors are a separate service on a private TLS link with no database route, and scale independently');
  assert.equal(compose(['exec','-T','api','id','-u']),'10001');
  assert.equal(compose(['exec','-T','executor','id','-u']),'10001');
  assert.equal(compose(['exec','-T','executor','/opt/ayati/goose','--version']),'1.54.0');
  compose(['exec','-T','api','sh','-c','test -z "${OPENAI_API_KEY_FILE-}"; test -z "${OPENAI_API_KEY-}"']);
  compose(['exec','-T','executor','sh','-c','test -z "${WORKOS_API_KEY_FILE-}"; test -z "${RELAY_DATABASE_URL_FILE-}"; test ! -e /tmp/secret-executed']);
  assert.throws(() => compose(['run','--rm','--no-deps','-e','OPENAI_API_KEY=fixture-conflict','executor','help']), /Configure either OPENAI_API_KEY or OPENAI_API_KEY_FILE/);
  assert.throws(() => compose(['run','--rm','--no-deps','-e','OPENAI_API_KEY_FILE=/missing-secret','executor','help']), /Cannot read OPENAI_API_KEY_FILE/);
  const executorId = compose(['ps','-q','executor']);
  assert.equal(docker(['inspect','--format','{{.HostConfig.ReadonlyRootfs}}',executorId]),'true');
  pass('Same image runs as UID 10001 with read-only root, pinned Goose, separate role secrets and pooled executor registration');
  for (const [service,role] of [['api','controller'],['executor','executor']]) {
    const lines = docker(['logs',compose(['ps','-q',service])],{stdio:['pipe','pipe','pipe']}).split('\n').filter(Boolean);
    const records = lines.map(line => JSON.parse(line)); // Every line is one JSON object.
    const start = records.find(record => record.message === 'Starting');
    assert.equal(start?.role,role);
    assert.ok(start?.release,'Startup line names the image release (unknown when not passed at build)');
    if (role === 'controller') {
      const metrics = records.find(record => record._aws?.CloudWatchMetrics);
      assert.equal(metrics?._aws.CloudWatchMetrics[0].Namespace,'Ayati');
      assert.ok(Number.isFinite(metrics?.QueuedWork) && Number.isFinite(metrics?.ReadySlots));
    }
  }
  pass('Containers log one JSON object per line, starting with their role and release');
  // Node fetch does not preserve a custom Host; use HTTP transport to emulate the TLS proxy.
  const get = (path,cookie='') => new Promise((resolve,reject) => {
    httpGet({host:'127.0.0.1',port,path,headers:{Host:'workspace.example.test',Cookie:cookie,Connection:'close'}},res => {
      let body = ''; res.setEncoding('utf8'); res.on('data',chunk => { body += chunk; });
      res.on('end',()=>resolve({status:res.statusCode,headers:res.headers,body}));
    }).on('error',reject);
  });
  const login = await get('/auth/login');
  assert.equal(login.status,303);
  const cookie = login.headers['set-cookie'][0].split(';')[0];
  assert.match(login.headers['set-cookie'][0],/; Secure/);
  assert.equal(sql('SELECT count(*) FROM relay_login_flows'),'1');
  compose(['rm','-sf','api']);
  compose(['up','-d','--no-deps','api']); await healthy('api');
  // The executor is not recreated: it reconnects to the replacement on its own.
  assert.equal(sql('SELECT count(*) FROM relay_login_flows'),'1');
  assert.equal((await get('/auth/callback?error=access_denied',cookie)).status,303);
  assert.equal(sql('SELECT count(*) FROM relay_login_flows'),'0');
  assert.equal((await get('/index.html')).status,200);
  pass('Replacing the API container loses scratch only; shared login state and packaged website remain available');
  await healthy('executor');
  const drained = compose(['ps','-q','executor']);
  compose(['stop','-t','20','executor']);
  assert.equal(docker(['inspect','--format','{{.State.ExitCode}}',drained]),'0');
  pass('The executor drains its prepared slots through the controller on SIGTERM');
  const stopped = compose(['ps','-q','api']);
  docker(['kill','--signal','TERM',stopped]);
  const draining = await get('/health/ready');
  assert.deepEqual([draining.status,draining.body],[503,'draining']);
  assert.equal((await get('/health/live')).status,200);
  await until(() => docker(['inspect','--format','{{.State.Running}}',stopped]) === 'false',
    'API did not stop after draining',30000);
  assert.equal(docker(['inspect','--format','{{.State.ExitCode}}',stopped]),'0');
  pass('A stopping API reports draining while still serving, then exits cleanly');
  compose(['up','-d','api']); await healthy('api');
  compose(['stop','-t','5','db']);
  assert.throws(() => compose(['exec','-T','api','/opt/ayati/ayati','api-health']));
  assert.equal((await get('/health/live')).status,200);
  pass('Database loss fails API readiness while liveness stays up');
});
