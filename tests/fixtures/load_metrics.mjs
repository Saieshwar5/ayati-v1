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
export function startPoints(app) {
  const found = new Map();
  for (const process of app.processes()) for (const line of process.logs.replace(/\x1b\[[0-9;]*m/g, '').split('\n')) {
    const match = line.match(/assignment="?([a-f0-9-]+)"?.*side="?(controller|executor)"?.*start_point=(\{.*\})/);
    if (!match) continue;
    const point = JSON.parse(match[3]);
    assert.equal(point.version, 1);
    assert.ok(Number.isSafeInteger(point.unix_us));
    const item = found.get(match[1]) ?? {};
    item[point.phase] ??= point.unix_us;
    found.set(match[1], item);
  }
  return found;
}
const intervals = {
  offer_delivery: ['offer_available', 'offer_received'],
  executor_queue: ['offer_received', 'start_http_begin'],
  start_request: ['start_http_begin', 'start_handler_enter'],
  controller_handler: ['start_handler_enter', 'start_handler_return'],
  start_response: ['start_handler_return', 'start_http_returned'],
  local_handoff: ['start_http_returned', 'acp_submit'],
  acp_to_provider_headers: ['acp_submit', 'provider_headers_us'],
  provider_body_transfer: ['provider_headers_us', 'provider_body_us'],
  provider_decode: ['provider_body_us', 'provider_request_us'],
};
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
  const gaps = Object.fromEntries(Object.entries(intervals).map(([name, [begin, end]]) =>
    [name, quantiles(samples.map(sample => (sample.points[end] - sample.points[begin]) / 1000))]));
  return { count: samples.length, distributions_ms: distributions, start_phases_us: phases,
    handoff_intervals_ms: gaps,
    negative_interval_samples: Object.fromEntries(Object.entries(intervals).map(([name, [begin, end]]) =>
      [name, samples.filter(sample => sample.points[end] < sample.points[begin]).length])),
    prompt_bytes: { min: Math.min(...samples.map(s => s.prompt_bytes)), max: Math.max(...samples.map(s => s.prompt_bytes)) },
    provisional_targets: { dispatch_p95_under_100: distributions.dispatch.p95 < 100, model_request_p95_under_250: distributions.fixture_model_request.p95 < 250 } };
}
