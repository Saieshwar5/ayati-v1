import { mkdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import type { Page, TestInfo } from '@playwright/test';
import type { TestDaemon } from './daemon.ts';
import type { ProviderCall } from './scripted-model.ts';
import type { Snapshot } from '../src/types.ts';

export class Evidence {
  readonly checks: { name: string; expected: unknown; actual: unknown }[] = [];
  readonly events: { at_ms: number; kind: string; detail: unknown }[] = [];
  readonly browserErrors: string[] = [];
  private started = performance.now();
  private key = process.env.FIREWORKS_API_KEY;

  observe(page: Page): void {
    const requests = new WeakMap<object, number>();
    page.on('request', (request) => {
      requests.set(request, performance.now());
      this.record('request', { method: request.method(), path: new URL(request.url()).pathname });
    });
    page.on('response', (response) => {
      this.record('response', {
        path: new URL(response.url()).pathname, status: response.status(),
        duration_ms: Math.round(performance.now() - (requests.get(response.request()) ?? performance.now())),
      });
      if (new URL(response.url()).pathname === '/api/state' && response.ok()) {
        void response.json().then((state: Snapshot) => this.record('observed_state', {
          tasks: state.tasks,
          messages: state.messages.map((message) => ({
            id: message.id, state: message.state, characters: message.content.length,
          })),
        })).catch(() => undefined);
      }
    });
    page.on('requestfailed', (request) => this.record('request_failed', {
      path: new URL(request.url()).pathname, error: request.failure()?.errorText,
    }));
    page.on('pageerror', (error) => this.browserErrors.push(error.message));
    page.on('console', (message) => {
      if (message.type() === 'error') this.record('console_error', message.text());
    });
  }

  record(kind: string, detail: unknown): void {
    // Long-lived event streams can trigger many requests. Keep evidence bounded.
    if (this.events.length < 1000) this.events.push({ at_ms: Math.round(performance.now() - this.started), kind, detail });
  }

  check(name: string, expected: unknown, actual: unknown): void {
    this.checks.push({ name, expected, actual });
  }

  async save(info: TestInfo, daemon: TestDaemon, calls: ProviderCall[]): Promise<void> {
    let database: Record<string, unknown> = {};
    let exportFailed = false;
    try {
      const db = new DatabaseSync(join(daemon.data, 'ayati.sqlite'), { readOnly: true });
      try {
        database = {
          tasks: db.prepare('SELECT id,status,detail,created_at,context FROM tasks ORDER BY rowid').all()
            .map((row) => ({ ...row, context: parse(String(row.context)) })),
          messages: db.prepare('SELECT * FROM messages ORDER BY rowid').all(),
          tools: db.prepare('SELECT * FROM tool_attempts ORDER BY rowid').all()
            .map((row) => ({ ...row, arguments: parse(String(row.arguments)), result: parse(String(row.result)) })),
          artifacts: db.prepare('SELECT * FROM artifacts ORDER BY rowid').all(),
        };
      } finally { db.close(); }
    } catch (error) {
      exportFailed = true;
      this.record('evidence_error', String(error));
    }
    const messages = database.messages as { role: string }[] | undefined;
    const result = this.redact({
      schema_version: 1, run_id: process.env.AYATI_TEST_RUN_DIR?.split('/').at(-1),
      scenario: info.title, retry: info.retry,
      status: exportFailed || this.events.some((event) => ['scenario_error', 'cleanup_error'].includes(event.kind))
        ? 'failed' : info.status,
      mode: process.env.AYATI_TEST_MODE, model: daemon.model,
      duration_ms: Math.round(performance.now() - this.started),
      model_turns: messages?.filter((message) => message.role === 'assistant').length ?? 0,
      token_usage: null, model_cost: null,
      usage_note: 'The current Ayati provider adapter does not expose usage or cost; no estimate is invented.',
      checks: this.checks, browser_errors: this.browserErrors, events: this.events,
      errors: (info.errors ?? []).map((error) => error.message ?? error.value),
      fixture_provider_calls: calls, ...database,
    });
    const path = info.outputPath('evidence.json');
    await mkdir(info.outputDir, { recursive: true });
    await writeFile(path, JSON.stringify(result, null, 2));
    await writeFile(info.outputPath('daemon.log'), String(this.redact(daemon.logs)));
    await info.attach('Ayati run evidence', { path, contentType: 'application/json' });
    await info.attach('Daemon log', { path: info.outputPath('daemon.log'), contentType: 'text/plain' });
    if (exportFailed) throw new Error('Could not export the test database; inspect evidence.json and daemon.log.');
  }

  private redact(value: unknown): unknown {
    if (typeof value === 'string') return this.key ? value.split(this.key).join('[REDACTED]') : value;
    if (Array.isArray(value)) return value.map((item) => this.redact(item));
    if (value && typeof value === 'object') return Object.fromEntries(
      Object.entries(value).filter(([key]) => key !== 'reasoning_content').map(([key, item]) => [key,
        /authorization|password|api_key|cookie/i.test(key) ? '[REDACTED]' : this.redact(item)]),
    );
    return value;
  }
}

function parse(text: string): unknown {
  try { return JSON.parse(text); } catch { return text; }
}
