import assert from 'node:assert/strict';
export function quantiles(values) {
  const sorted = [...values].sort((a, b) => a - b);
  assert.ok(sorted.length > 0 && sorted.every(Number.isFinite), 'Missing/nonfinite measurement');
  return Object.fromEntries([50, 95, 99].map(p => ['p' + p, sorted[Math.ceil(p * sorted.length / 100) - 1]]));
}
export function traces(app) {
  const found = new Map();
  for (const process of app.processes()) for (const line of process.logs.replace(/\x1b\[[0-9;]*m/g, '').split('\n')) {
    const match = line.match(/assignment="?([a-f0-9-]+)"?.*side="?(controller|executor)"?.*start_trace=(\{.*\})/);
    if (!match) continue;
    const item = found.get(match[1]) ?? {};
    item[match[2]] ??= JSON.parse(match[3]); found.set(match[1], item);
  }
  return found;
}
export function summarize(samples) {
  const distributions = {};
  for (const [name, key] of Object.entries({ client_ack: 'ack_ms', dispatch: 'reserved_ms', context: 'context_ready_ms', fixture_model_request: 'provider_request_ms', fixture_first_token: 'provider_first_token_ms', saved_text: 'first_output_ms', final: 'finished_ms' })) {
    distributions[name] = quantiles(samples.map(s => s[key] - (key === 'ack_ms' ? s.sent_ms : s.accepted_ms)));
  }
  const phases = {};
  for (const side of ['controller', 'executor']) {
    const labels = new Set(samples.flatMap(s => Object.keys(s.trace[side].phases_us)));
    phases[side] = Object.fromEntries([...labels].map(label => [label, quantiles(samples.map(s => s.trace[side].phases_us[label]).filter(Number.isFinite))]));
  }
  return { count: samples.length, distributions_ms: distributions, start_phases_us: phases,
    prompt_bytes: { min: Math.min(...samples.map(s => s.prompt_bytes)), max: Math.max(...samples.map(s => s.prompt_bytes)) },
    provisional_targets: { dispatch_p95_under_100: distributions.dispatch.p95 < 100, model_request_p95_under_250: distributions.fixture_model_request.p95 < 250 } };
}
