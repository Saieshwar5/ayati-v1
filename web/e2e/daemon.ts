import { spawn, type ChildProcess } from 'node:child_process';
import { once } from 'node:events';
import { createServer } from 'node:net';
import { mkdir } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import type { Snapshot } from '../src/types.ts';

export const projectRoot = resolve(import.meta.dirname, '../..');

export class TestDaemon {
  readonly data: string;
  readonly providerURL?: string;
  base = '';
  logs = '';
  model = '';
  private port = 0;
  private child?: ChildProcess;

  constructor(data: string, providerURL?: string) {
    this.data = data;
    this.providerURL = providerURL;
  }

  async start(): Promise<void> {
    await mkdir(this.data, { recursive: true, mode: 0o700 });
    if (!this.port) this.port = await availablePort();
    this.base = `http://127.0.0.1:${this.port}`;
    const env = { ...process.env, AYATI_DATA_DIR: this.data, AYATI_PORT: String(this.port) };
    if (this.providerURL) Object.assign(env, {
      AYATI_MODEL_BASE_URL: this.providerURL, AYATI_MODEL: 'scripted-fixture',
      FIREWORKS_API_KEY: 'fixture-only-placeholder', AYATI_CONTEXT_BYTES: '96000',
    });
    this.child = spawn(join(projectRoot, 'target/release/ayati'), ['serve'], {
      cwd: projectRoot, env, stdio: ['ignore', 'pipe', 'pipe'],
    });
    let spawnError: Error | undefined;
    this.child.on('error', (error) => { spawnError = error; });
    const append = (chunk: Buffer) => { this.logs = (this.logs + chunk.toString()).slice(-64_000); };
    this.child.stdout?.on('data', append);
    this.child.stderr?.on('data', append);
    const deadline = Date.now() + 10_000;
    while (Date.now() < deadline) {
      if (spawnError) throw spawnError;
      if (this.child.exitCode !== null || this.child.signalCode !== null) {
        throw new Error('Test daemon exited during startup; inspect daemon.log in the evidence.');
      }
      try {
        const response = await fetch(`${this.base}/api/health`, { signal: AbortSignal.timeout(500) });
        if (response.ok) {
          const health = await response.json() as { model: string; model_ready: boolean };
          if (!health.model_ready) throw new Error('Test daemon has no model connection.');
          this.model = health.model;
          return;
        }
      } catch { /* Poll only during daemon startup. */ }
      await delay(25);
    }
    throw new Error('Test daemon did not become ready; inspect daemon.log in the evidence.');
  }

  async state(): Promise<Snapshot> {
    const response = await fetch(`${this.base}/api/state`, { signal: AbortSignal.timeout(2000) });
    if (!response.ok) throw new Error(`Test snapshot returned HTTP ${response.status}`);
    return response.json() as Promise<Snapshot>;
  }

  async stop(): Promise<void> {
    const child = this.child;
    if (!child || child.exitCode !== null || child.signalCode !== null) return;
    const exited = once(child, 'exit');
    try {
      const response = await fetch(`${this.base}/api/shutdown`, {
        method: 'POST', signal: AbortSignal.timeout(2000),
      });
      if (!response.ok) throw new Error(`Shutdown returned HTTP ${response.status}`);
      await within(exited, 7000);
      if (child.exitCode !== 0) throw new Error('Test daemon exited unsuccessfully.');
    } catch (error) {
      child.kill('SIGKILL');
      await within(exited, 2000).catch(() => undefined);
      throw error;
    }
  }

  async restart(): Promise<void> {
    await this.stop();
    await this.start();
  }
}

async function availablePort(): Promise<number> {
  const server = createServer();
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('Could not reserve a test port');
  await new Promise<void>((done, reject) => server.close((error) => error ? reject(error) : done()));
  return address.port;
}

async function within<T>(promise: Promise<T>, milliseconds: number): Promise<T> {
  let timer: NodeJS.Timeout | undefined;
  try {
    return await Promise.race([promise, new Promise<never>((_, reject) => {
      timer = setTimeout(() => reject(new Error('Test daemon shutdown timed out')), milliseconds);
    })]);
  } finally { clearTimeout(timer); }
}
