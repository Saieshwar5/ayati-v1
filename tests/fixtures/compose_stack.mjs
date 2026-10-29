// Disposable Compose stacks for container tests: synthetic secrets, a private
// worker-control TLS set, a throwaway PostgreSQL, migration and runtime grants.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { randomBytes, randomUUID } from 'node:crypto';
import { once } from 'node:events';
import { mkdir, mkdtemp, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { join, resolve } from 'node:path';
import { until } from './polling.mjs';

export const root = resolve(import.meta.dirname, '../..');

export async function freePort() {
  const listener = createServer().listen(0, '127.0.0.1');
  await once(listener, 'listening');
  const { port } = listener.address();
  await new Promise(done => listener.close(done));
  return port;
}

/**
 * `files` are extra Compose overlays from the repository; `services` is YAML
 * appended to the test-only override (indented under `services:`); `env`
 * adds or replaces Compose interpolation values, optionally from the project name.
 */
export async function composeStack(t, { prefix, files = [], services = '', env: extra = {} }) {
  await mkdir(join(root, '.tmp'), { recursive: true });
  const data = await mkdtemp(join(root, `.tmp/${prefix}-`));
  const name = 'ayati-test-' + randomUUID().slice(0, 8);
  const port = await freePort();
  const secret = async (file, value) => {
    const path = join(data, file);
    // Synthetic files in a private directory; readable by the image's non-root UID.
    await writeFile(path, value, { mode: 0o444 }); return path;
  };
  // Private worker-control TLS from the operator tool; the server key is readable
  // by the image's UID only through control-key's private install.
  const tlsDir = join(data, 'worker-control-tls');
  execFileSync(join(root, 'tools/worker-control-certs.sh'), [tlsDir, 'worker-control'], { stdio: 'ignore' });
  execFileSync('chmod', ['0444', join(tlsDir, 'server.key')]);
  const env = {
    AYATI_IMAGE: process.env.AYATI_CONTAINER_IMAGE ?? 'ayati:hosted-local',
    AYATI_PUBLIC_ORIGIN: 'https://workspace.example.test', AYATI_LOCAL_PORT: String(port),
    AYATI_WORKER_PROVIDERS: 'openai', AYATI_MODEL: 'fixture', WORKOS_CLIENT_ID: 'client_fixture',
    AYATI_DRAIN_SECONDS: '3',
    AYATI_DATABASE_URL_FILE: await secret('database-url', 'postgresql://relay_app:fixture-runtime@db:5432/ayati'),
    AYATI_MIGRATION_DATABASE_URL_FILE: await secret('migration-url', 'postgresql://postgres:fixture-admin@db:5432/ayati'),
    AYATI_WORKOS_API_KEY_FILE: await secret('identity-key', 'fixture-unused-key'),
    AYATI_MODEL_API_KEY_FILE: await secret('model-key', '$(touch /tmp/secret-executed)'),
    // control-key hands this to UID 10001 with mode 0600 inside private volumes.
    AYATI_WORKER_CONTROL_KEY_FILE: await secret('worker-control-key', randomBytes(32).toString('base64url')),
    AYATI_WORKER_CONTROL_TLS_DIR: tlsDir,
    ...(typeof extra === 'function' ? extra(name) : extra),
  };
  const envFile = join(data, '.env');
  await writeFile(envFile, Object.entries(env).map(([k, v]) => `${k}=${v}`).join('\n'));
  const override = join(data, 'rehearsal.yaml');
  await writeFile(override, `services:
  db:
    image: postgres:18.6@sha256:edb0c08dc33befd0262ab3b5cf1c4f3f8cfd8b09e88c05b45adee29d6761ddb6
    environment:
      POSTGRES_DB: ayati
      POSTGRES_PASSWORD: fixture-admin
    tmpfs: [/var/lib/postgresql]
${services}`);
  const args = ['compose', '--project-name', name, '--env-file', envFile, '-f', join(root, 'deploy/compose.yaml'),
    ...files.flatMap(file => ['-f', join(root, file)]), '-f', override];
  const docker = (input, options = {}) => execFileSync('docker', input, {
    encoding: 'utf8', timeout: 90000, maxBuffer: 2 * 1024 * 1024, stdio: ['pipe', 'pipe', 'pipe'], ...options,
  }).trim();
  const compose = (input, options) => docker([...args, ...input], options);
  const sql = input => compose(['exec', '-T', 'db', 'psql', '-U', 'postgres', '-d', 'ayati', '-At', '-v', 'ON_ERROR_STOP=1'], { input });
  const checks = [];
  const pass = check => { checks.push(check); console.log('PASS', check); };
  t.after(async () => {
    try { await writeFile(join(data, 'containers.log'), compose(['logs', '--no-color'])); } catch {}
    try { compose(['--profile', 'maintenance', 'down', '--volumes', '--remove-orphans']); }
    finally {
      await writeFile(join(data, 'report.json'), JSON.stringify({ passed: t.passed, checks }, null, 2));
      console.log('Retained container evidence:', data);
    }
  });
  const healthy = service => until(() => {
    const ids = compose(['ps', '-q', service]).split('\n').filter(Boolean);
    return ids.length > 0 && ids.every(id => docker(['inspect', '--format', '{{.State.Health.Status}}', id]) === 'healthy');
  }, service + ' container did not become healthy', 60000);
  /** Start the database, run the explicit migration and apply runtime grants. */
  async function migrate() {
    compose(['config', '--quiet']);
    compose(['up', '-d', 'db']);
    await until(() => { try { compose(['exec', '-T', 'db', 'pg_isready', '-h', '127.0.0.1', '-U', 'postgres']); return true; } catch { return false; } }, 'Fixture database did not start', 30000);
    sql("CREATE ROLE relay_app LOGIN PASSWORD 'fixture-runtime';");
    compose(['--profile', 'maintenance', 'run', '--rm', 'migrate']);
    sql(`GRANT USAGE ON SCHEMA public TO relay_app;
      GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA public TO relay_app;
      GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA public TO relay_app;
      REVOKE UPDATE,DELETE ON relay_audit FROM relay_app;
      REVOKE INSERT,UPDATE,DELETE ON seaql_migrations FROM relay_app;`);
    assert.equal(sql("SELECT has_schema_privilege('relay_app','public','CREATE')"), 'f');
  }
  return { root, data, name, port, env, compose, docker, sql, pass, healthy, migrate };
}
