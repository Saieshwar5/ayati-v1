import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { once } from 'node:events';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { until } from './fixtures/polling.mjs';

const options = { skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 120000 };
const ACCOUNT = '123456789012', TASK = '0123456789abcdef0123456789abcdef';
const CLUSTER = `arn:aws:ecs:ap-south-1:${ACCOUNT}:cluster/ayati-test`;
const RELEASE = `arn:aws:ecs:ap-south-1:${ACCOUNT}:task-definition/ayati-test:1`;
const ROLE = `arn:aws:iam::${ACCOUNT}:role/ayati-test-executor`;

/** Loopback stand-in for the ECS credential endpoint, STS and ECS. Signature
 * verification is covered by the Rust tests; this checks shape and binding. */
async function fakeAws() {
  const state = { status: 'RUNNING', proofs: 0 };
  const server = createServer((request, response) => {
    let body = ''; request.on('data', chunk => { body += chunk; });
    request.on('end', () => {
      const send = (status, value, type = 'application/x-amz-json-1.1') => {
        response.writeHead(status, { 'Content-Type': type }); response.end(typeof value === 'string' ? value : JSON.stringify(value));
      };
      if (request.method === 'GET' && request.url === '/credentials') return send(200,
        { AccessKeyId: 'AKIDTESTEXECUTOR', SecretAccessKey: 'synthetic-secret', Token: 'synthetic-session',
          Expiration: new Date(Date.now() + 3600e3).toISOString() }, 'application/json');
      const target = request.headers['x-amz-target'] ?? '';
      if (target.endsWith('.DescribeTasks')) return send(200, { failures: [], tasks: [{ taskArn: `arn:aws:ecs:ap-south-1:${ACCOUNT}:task/ayati-test/${TASK}`,
        clusterArn: CLUSTER, lastStatus: state.status, taskDefinitionArn: RELEASE }] });
      if (target.endsWith('.DescribeTaskDefinition')) return send(200, { taskDefinition: { taskRoleArn: ROLE } });
      const signed = /SignedHeaders=[^,]*x-ayati-registration/.test(request.headers.authorization ?? '');
      if (body !== 'Action=GetCallerIdentity&Version=2011-06-15' || !signed || request.headers.host !== 'sts.ap-south-1.amazonaws.com')
        return send(403, '<Error/>', 'text/xml');
      state.proofs += 1;
      send(200, `<GetCallerIdentityResponse><GetCallerIdentityResult><Arn>arn:aws:sts::${ACCOUNT}:assumed-role/ayati-test-executor/${TASK}</Arn></GetCallerIdentityResult></GetCallerIdentityResponse>`, 'text/xml');
    });
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  return { state, url: `http://127.0.0.1:${server.address().port}`, close: () => server.close() };
}

test('an executor registers with its proven AWS task identity and no bootstrap key', options, async t => {
  const aws = await fakeAws(); t.after(() => aws.close());
  const app = await workerFixture(t, { mode: 'pooled', control: true, tls: true, apiEnv: {
    AYATI_CLOUD_WORKERS: '1', AYATI_AWS_EXECUTOR_CLUSTER_ARN: CLUSTER, AYATI_AWS_EXECUTOR_RELEASE_ARN: RELEASE,
    AYATI_AWS_EXECUTOR_ROLE_ARN: ROLE, AYATI_AWS_TEST_ENDPOINT: aws.url,
    AWS_ACCESS_KEY_ID: 'AKIDTESTCONTROLLER', AWS_SECRET_ACCESS_KEY: 'synthetic-controller-secret' } });
  const cloud = { AYATI_EXECUTOR_IDENTITY: 'aws', AYATI_WORKER_CONTROL_KEY_FILE: undefined,
    AWS_CONTAINER_CREDENTIALS_FULL_URI: `${aws.url}/credentials` };
  aws.state.status = 'STOPPED';
  const stopped = await app.startExecutor(cloud);
  await until(() => stopped.child.exitCode !== null, 'An executor of a stopped task kept running', 30000);
  assert.notEqual(stopped.child.exitCode, 0);
  assert.equal(app.sql('SELECT count(*) FROM relay_cloud_executors'), '0');
  aws.state.status = 'RUNNING';
  const executor = await app.startExecutor(cloud);
  await until(() => Number(app.sql("SELECT count(*) FROM relay_executor_slots WHERE state='ready'")) > 0,
    'The cloud executor did not report ready slots', 35000);
  assert.equal(app.sql('SELECT task_arn FROM relay_cloud_executors'), `arn:aws:ecs:ap-south-1:${ACCOUNT}:task/ayati-test/${TASK}`);
  const job = await app.send('A request served by a proven cloud executor');
  await app.done(job);
  await app.stop(executor);
  app.pass('ECS confirms the task before registration; a proven executor serves a reply with no shared bootstrap key');
});
