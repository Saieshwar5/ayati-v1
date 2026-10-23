#!/usr/bin/env node
import { parseArgs } from 'node:util';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdirSync, mkdtempSync, chmodSync, writeFileSync, readFileSync, openSync, closeSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';
import { loadPlan, readPrivateKey, scriptedPlan, validatePlan } from '../tests/calibration/plan.mjs';
import { Budget } from '../tests/calibration/budget.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
async function main() {
  const { values } = parseArgs({ options: { image: { type: 'string' }, plan: { type: 'string' },
    live: { type: 'boolean', default: false }, 'key-file': { type: 'string' },
    check: { type: 'boolean', default: false }, help: { type: 'boolean', default: false } } });
  if (values.help) {
    console.log('Usage: node tools/calibrate-provider.mjs --image sha256:... [--check]\n'
      + 'Scripted rehearsal is the default. Live mode additionally requires\n'
      + '--live --plan /private/plan.json --key-file /private/provider.key.\n'
      + 'See runtime/provider-calibration.md for accounting limits and plan fields.');
    return 0;
  }
  assert.match(values.image ?? '', /^sha256:[a-f0-9]{64}$/, 'Supply --image with the immutable verified local image ID');
  assert.ok(!values.live || (values.plan && values['key-file']), 'Live mode requires --plan and --key-file');
  assert.ok(values.live || !values['key-file'], 'Scripted rehearsal does not accept a real key');
  const loaded = values.plan ? loadPlan(resolve(values.plan), values.live)
    : { plan: validatePlan(scriptedPlan()), sha256: null };
  const budget = new Budget(loaded.plan);
  assert.ok(budget.perCall * 7n <= budget.limit, 'The declared allowance must cover all seven possible requests');
  if (values.live) readPrivateKey(resolve(values['key-file']));
  const preflight = { mode: values.live ? 'live' : 'scripted', image: values.image,
    plan: loaded.plan, plan_sha256: loaded.sha256, required_requests: 7,
    seven_call_reservation_micro_usd: (budget.perCall * 7n).toString(),
    maximum_forwarded_calls: Math.min(loaded.plan.max_requests, Number(budget.limit / budget.perCall)) };
  if (values.check) { console.log(JSON.stringify(preflight, null, 2)); return 0; }
  mkdirSync(join(root, '.tmp'), { recursive: true });
  const evidence = mkdtempSync(join(root, '.tmp/provider-calibration-'));
  chmodSync(evidence, 0o700);
  const config = { plan: loaded.plan, live: values.live, evidence,
    ...(values.live && { key_file: resolve(values['key-file']) }) };
  const configFile = join(evidence, 'config.json');
  writeFileSync(configFile, JSON.stringify(config) + '\n', { mode: 0o600 });
  const report = { ...preflight, passed: false, began_utc: new Date().toISOString(),
    charges: values.live ? 'Usage estimates from declared rates; provider invoice remains authoritative'
      : 'No provider charges; scripted accounting only' };
  const save = () => writeFileSync(join(evidence, 'verification.json'), JSON.stringify(report, null, 2) + '\n', { mode: 0o600 });
  save(); console.log('Retained provider calibration:', evidence);
  const log = openSync(join(evidence, 'run.log'), 'w', 0o600);
  try {
    const child = spawn('python3', ['tools/run-agent-load.py', '--suite', 'calibration',
      '--image', values.image, '--executor-cpu', '200'], { cwd: root,
      env: { ...process.env, AYATI_CALIBRATION_CONFIG: configFile },
      detached: true, stdio: ['ignore', log, log] });
    const interrupted = () => {
      if (!report.interrupted) { report.interrupted = true; child.kill('SIGINT'); }
    };
    process.on('SIGINT', interrupted); process.on('SIGTERM', interrupted);
    try {
      const [code] = await once(child, 'exit'); report.exit_code = code;
      const output = readFileSync(join(evidence, 'run.log'), 'utf8');
      const pointer = output.match(/^Retained bounded rehearsal: (.+)$/m)?.[1];
      report.bounded_evidence = pointer;
      assert.ok(pointer, 'Bounded runner did not produce evidence');
      const bounded = JSON.parse(readFileSync(join(pointer, 'result.json')));
      assert.equal(bounded.image.id, values.image); assert.deepEqual(bounded.errors, []);
      assert.equal(code, 0, 'Calibration incomplete; inspect retained reports');
      const measured = JSON.parse(readFileSync(join(evidence, 'calibration-report.json')));
      assert.ok(measured.passed && measured.mode === preflight.mode);
      report.passed = true;
      report.sample_count = measured.sample_count;
      report.distributions_ms = measured.distributions_ms;
      report.reserved_micro_usd = measured.ledger.reserved_micro_usd;
      report.usage_cost_estimate_micro_usd = measured.ledger.usage_cost_estimate_micro_usd;
      report.unknown_usage_requests = measured.ledger.unknown_usage_requests;
    } finally { process.off('SIGINT', interrupted); process.off('SIGTERM', interrupted); }
  } catch { report.error = 'Calibration incomplete; inspect private run.log, calibration-report.json and bounded evidence'; }
  finally { closeSync(log); report.finished_utc = new Date().toISOString(); save(); }
  console.log('Calibration:', report.passed ? 'passed' : 'incomplete', '(' + preflight.mode + ')');
  return report.passed ? 0 : 1;
}
main().then(code => { process.exitCode = code; }).catch(() => {
  console.error('Calibration configuration refused; check image, plan fields, current rates, allowance and private key-file permissions.');
  process.exitCode = 1;
});
