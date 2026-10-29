import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer as tlsServer, get as httpsGet } from 'node:https';
import { request as httpRequest } from 'node:http';
import { createServer } from 'node:net';
import { once } from 'node:events';
import { execFileSync } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { until } from './fixtures/polling.mjs';

test('HTTPS gateway and durable login state survive a real API process replacement', {
  timeout: 90000, skip: !process.env.RELAY_TEST_DATABASE_URL,
}, async t => {
  const fixture = await workerFixture(t, { mode: 'pooled', control: true });
  await fixture.stopApi();
  const listener = createServer().listen(0, '127.0.0.1');
  await once(listener, 'listening');
  const port = listener.address().port;
  await new Promise(resolve => listener.close(resolve));
  const keyPath = join(fixture.data, 'tls-fixture.key');
  const certPath = join(fixture.data, 'tls-fixture.pem');
  execFileSync('openssl', ['req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-days', '1',
    '-subj', '/CN=localhost', '-addext', 'subjectAltName=DNS:localhost,IP:127.0.0.1',
    '-keyout', keyPath, '-out', certPath], { stdio: 'pipe' });
  const proxy = tlsServer({ key: await readFile(keyPath), cert: await readFile(certPath) }, (req, res) => {
    const upstream = httpRequest({ host: '127.0.0.1', port, path: req.url, method: req.method,
      headers: { ...req.headers, 'x-forwarded-proto': 'https' } }, result => {
      res.writeHead(result.statusCode, result.headers); result.pipe(res);
    });
    upstream.on('error', () => { res.writeHead(502); res.end(); });
    req.pipe(upstream);
  }).listen(0, '127.0.0.1');
  await once(proxy, 'listening');
  t.after(() => new Promise(resolve => proxy.close(resolve)));
  const origin = `https://127.0.0.1:${proxy.address().port}`;
  const ca = await readFile(certPath);
  // Trust only this local fixture certificate, never disable TLS validation globally.
  function get(path, cookie = '') {
    return new Promise((resolve, reject) => {
      httpsGet(origin + path, { ca, headers: { Cookie: cookie, Connection: 'close' } }, response => {
          let body = ''; response.on('data', chunk => { body += chunk; });
          response.on('end', () => resolve({ status: response.statusCode, headers: response.headers, body }));
      }).on('error', reject);
    });
  }
  const config = { AYATI_PUBLIC_ORIGIN: origin, AYATI_PORT: String(port), AYATI_BIND_ADDRESS: '127.0.0.1' };
  let api = fixture.startHosted(config);
  await until(async () => { assert.equal(api.child.exitCode, null, api.logs); return (await get('/health/ready')).status === 200; }, 'Hosted API not ready');
  const login = await get('/auth/login');
  assert.equal(login.status, 303);
  assert.match(login.headers['set-cookie'][0], /; Secure/);
  const flowCookie = login.headers['set-cookie'][0].split(';')[0];
  const authorization = new URL(login.headers.location);
  assert.equal(authorization.searchParams.get('redirect_uri'), origin + '/auth/callback');
  assert.equal(fixture.sql('SELECT count(*) FROM relay_login_flows'), '1');
  await fixture.stop(api);
  api = fixture.startHosted(config); // A new scratch directory, the same permanent database.
  await until(async () => (await get('/health/ready')).status === 200, 'Replacement API not ready');
  assert.equal(fixture.sql('SELECT count(*) FROM relay_login_flows'), '1');
  const callback = await get('/auth/callback?error=access_denied&state=' + authorization.searchParams.get('state'), flowCookie);
  assert.equal(callback.status, 303);
  assert.equal(fixture.sql('SELECT count(*) FROM relay_login_flows'), '0');
  assert.match(callback.headers['set-cookie'][0], /Max-Age=0; Secure/);
  const asset = await get('/index.html');
  assert.equal(asset.status, 200); assert.match(asset.body, /<html/);
  const forbidden = await fetch(`http://127.0.0.1:${port}/api/identity/session`);
  assert.equal(forbidden.status, 403); // A forwarded header cannot replace the configured Host.
  await fixture.stop(api);
  fixture.pass('Private HTTPS proxy, Secure cookies, public callback, durable login flow and static client verified across API process replacement');
});
