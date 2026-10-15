import type { FullResult, Reporter, Suite, TestCase, TestResult } from '@playwright/test/reporter';
import { readFile, writeFile } from 'node:fs/promises';
import { join, relative } from 'node:path';

type Attempt = {
  retry: number; status: string; duration_ms: number; errors: string[];
  evidence?: string;
};

export default class EvidenceReporter implements Reporter {
  private directory = process.env.AYATI_TEST_RUN_DIR!;
  private suite?: Suite;
  private attempts = new Map<string, Attempt[]>();

  onBegin(_config: unknown, suite: Suite): void { this.suite = suite; }

  onTestEnd(test: TestCase, result: TestResult): void {
    const attachment = result.attachments.find((item) => item.name === 'Ayati run evidence');
    const attempt: Attempt = {
      retry: result.retry, status: result.status, duration_ms: result.duration,
      errors: result.errors.map((error) => this.redact(error.message ?? error.value ?? 'Unknown test failure')),
      evidence: attachment?.path ? relative(this.directory, attachment.path) : undefined,
    };
    this.attempts.set(test.id, [...this.attempts.get(test.id) ?? [], attempt]);
  }

  async onEnd(result: FullResult): Promise<void> {
    const manifest = JSON.parse(await readFile(join(this.directory, 'manifest.json'), 'utf8')) as Record<string, unknown>;
    const scenarios = this.suite?.allTests().map((test) => ({
      title: test.title, outcome: test.outcome(), expected_status: test.expectedStatus,
      attempts: this.attempts.get(test.id) ?? [],
    })) ?? [];
    const summary = {
      ...manifest, status: result.status, duration_ms: result.duration,
      passed: scenarios.filter((scenario) => scenario.outcome === 'expected').length,
      failed: scenarios.filter((scenario) => scenario.outcome === 'unexpected').length,
      flaky: scenarios.filter((scenario) => scenario.outcome === 'flaky').length,
      skipped: scenarios.filter((scenario) => scenario.outcome === 'skipped').length,
      scenarios,
    };
    await writeFile(join(this.directory, 'summary.json'), JSON.stringify(summary, null, 2));
    const lines = [
      '# Ayati browser test run', '',
      `Run: ${manifest.id}`, `Mode: ${manifest.mode}`, `Model: ${manifest.model}`,
      `Git: ${manifest.git_commit} (working tree dirty: ${manifest.working_tree_dirty})`,
      `Result: **${summary.status}** — ${summary.passed} passed, ${summary.failed} failed, ${summary.flaky} flaky, ${summary.skipped} skipped.`,
      `Duration: ${(summary.duration_ms / 1000).toFixed(1)} seconds.`, '',
      '[Browser report](html/index.html) · [Full Playwright results](playwright.json)', '',
    ];
    for (const scenario of scenarios) {
      lines.push(`## ${scenario.title}`, '', `Outcome: ${scenario.outcome}`, '');
      for (const attempt of scenario.attempts) {
        lines.push(`Attempt ${attempt.retry + 1}: ${attempt.status}, ${attempt.duration_ms} ms.`);
        if (attempt.evidence) lines.push(`[Messages, tools, files, timings and checks](${attempt.evidence})`);
        for (const error of attempt.errors) lines.push('', '```text', error, '```');
        lines.push('');
      }
    }
    lines.push('Live-model passes establish only the checks listed in their evidence. Human review remains necessary for usefulness and writing quality.', '');
    await writeFile(join(this.directory, 'summary.md'), lines.join('\n'));
  }

  private redact(text: string): string {
    const key = process.env.FIREWORKS_API_KEY;
    return key ? text.split(key).join('[REDACTED]') : text;
  }
}
