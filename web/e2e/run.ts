import { execFileSync, spawn } from 'node:child_process';
import { once } from 'node:events';
import { access, chmod, mkdir, readFile, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { loadEnvFile } from 'node:process';
import { randomUUID } from 'node:crypto';
import { retainCompletedRuns } from './run-history.ts';

const root = resolve(import.meta.dirname, '../..');
const runs = join(root, '.test-runs');
const cli = join(root, 'web/node_modules/@playwright/test/cli.js');
const args = process.argv.slice(2);

if (args.includes('--report')) {
  const latest = JSON.parse(await readFile(join(runs, 'latest.json'), 'utf8')) as { directory: string };
  const report = spawn(process.execPath, [cli, 'show-report', join(latest.directory, 'html')], { stdio: 'inherit' });
  const [code] = await once(report, 'exit');
  process.exitCode = Number(code ?? 1);
} else {
  const live = args.includes('--live');
  if (live) {
    try { loadEnvFile(join(root, '.env')); }
    catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; }
    if (!process.env.FIREWORKS_API_KEY?.trim()) {
      throw new Error('Live tests require FIREWORKS_API_KEY in the environment or local .env. No live tests were run.');
    }
  }
  await access(join(root, 'target/release/ayati')).catch(() => {
    throw new Error('Build the test binary first: cargo build --release');
  });
  await access(join(root, 'web/dist/index.html')).catch(() => {
    throw new Error('Build the web client first: npm run build --prefix web');
  });
  await mkdir(runs, { recursive: true, mode: 0o700 });
  await chmod(runs, 0o700);
  const id = `run-${new Date().toISOString().replace(/[:.]/g, '-')}-${randomUUID().slice(0, 8)}`;
  const directory = join(runs, id);
  await mkdir(directory, { mode: 0o700 });
  const manifest = {
    kind: 'ayati-browser-test', id, directory, mode: live ? 'live' : 'fixture', started_at: new Date().toISOString(),
    git_commit: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim(),
    working_tree_dirty: Boolean(execFileSync('git', ['status', '--porcelain'], { cwd: root, encoding: 'utf8' }).trim()),
    model: live ? process.env.AYATI_MODEL ?? 'accounts/fireworks/models/glm-5p3-flash' : 'scripted-fixture',
    node: process.version, playwright: '1.63.0',
  };
  await writeFile(join(directory, 'manifest.json'), JSON.stringify(manifest, null, 2));
  console.log(`Ayati ${manifest.mode} test run: ${directory}`);
  const child = spawn(process.execPath, [cli, 'test', ...args.filter((arg) => arg !== '--live')], {
    cwd: join(root, 'web'), stdio: ['inherit', 'pipe', 'pipe'],
    env: { ...process.env, AYATI_TEST_RUN_DIR: directory, AYATI_TEST_MODE: manifest.mode },
  });
  let output = '';
  const redact = (text: string) => process.env.FIREWORKS_API_KEY
    ? text.split(process.env.FIREWORKS_API_KEY).join('[REDACTED]') : text;
  const recordOutput = (chunk: Buffer, destination: NodeJS.WriteStream) => {
    output = (output + chunk.toString()).slice(-64_000);
    destination.write(redact(chunk.toString()));
  };
  child.stdout.on('data', (chunk: Buffer) => recordOutput(chunk, process.stdout));
  child.stderr.on('data', (chunk: Buffer) => recordOutput(chunk, process.stderr));
  const interrupt = () => child.kill('SIGINT');
  process.on('SIGINT', interrupt);
  const [code, signal] = await once(child, 'close');
  process.removeListener('SIGINT', interrupt);
  await writeFile(join(directory, 'runner.log'), redact(output));
  let exitCode = Number(code ?? 1);
  try { await access(join(directory, 'summary.json')); }
  catch {
    exitCode = 1;
    await writeFile(join(directory, 'summary.json'), JSON.stringify({
      ...manifest, status: 'failed', exit_code: code, signal, scenarios: [],
      error: 'The test runner exited before producing scenario reports; inspect runner.log.',
    }, null, 2));
    await writeFile(join(directory, 'summary.md'), '# Ayati browser test run\n\nThe runner exited before producing scenario reports. Inspect runner.log and manifest.json.\n');
  }
  await writeFile(join(directory, 'manifest.json'), JSON.stringify({
    ...manifest, finished_at: new Date().toISOString(), exit_code: exitCode, runner_exit_code: code, signal,
  }, null, 2));
  await writeFile(join(runs, 'latest.json'), JSON.stringify({ id, directory }, null, 2));
  await retainCompletedRuns(runs, 20);
  console.log(`Evidence: ${join(directory, 'summary.md')}`);
  console.log('Open the browser report: npm run test:report --prefix web');
  process.exitCode = exitCode;
}
