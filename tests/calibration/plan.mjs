import assert from 'node:assert/strict';
import { readFileSync, statSync } from 'node:fs';
import { createHash } from 'node:crypto';

export const endpoints = {
  openai: 'https://api.openai.com/v1/chat/completions',
  fireworks: 'https://api.fireworks.ai/inference/v1/chat/completions',
};

export function microDollars(value) {
  assert.equal(typeof value, 'string', 'USD values must be decimal strings');
  assert.match(value, /^\d{1,6}(\.\d{1,6})?$/, 'Use a positive USD decimal with at most six places');
  const [whole, fraction = ''] = value.split('.');
  return BigInt(whole) * 1000000n + BigInt(fraction.padEnd(6, '0'));
}

export function validatePlan(value, live = false) {
  assert.equal(typeof live, 'boolean', 'Live execution must be an explicit boolean');
  const fields = ['version', 'provider', 'model', 'max_requests', 'concurrency',
    'max_prompt_bytes', 'input_token_allowance', 'max_output_tokens', 'timeout_seconds',
    'budget_usd', 'input_usd_per_million', 'output_usd_per_million', 'pricing_source', 'pricing_checked'];
  assert.deepEqual(Object.keys(value).sort(), fields.sort(), 'Unexpected or missing plan fields');
  assert.equal(value.version, 1);
  assert.ok(live ? Object.hasOwn(endpoints, value.provider) : value.provider === 'scripted',
    'Live execution needs an explicitly selected provider; rehearsal uses scripted only');
  assert.match(value.model, /^[a-zA-Z0-9][a-zA-Z0-9._:/-]{0,199}$/);
  assert.ok(!/REPLACE|your-model|CHOOSE/i.test(value.model), 'Select an actual model');
  for (const [field, min, max] of [
    ['max_requests', 7, 24], ['concurrency', 2, 4], ['max_prompt_bytes', 24000, 131072],
    ['input_token_allowance', 32768, 2097152], ['max_output_tokens', 128, 2048],
    ['timeout_seconds', 10, 120],
  ]) assert.ok(Number.isSafeInteger(value[field]) && value[field] >= min && value[field] <= max, `Invalid ${field}`);
  for (const field of ['budget_usd', 'input_usd_per_million', 'output_usd_per_million']) {
    assert.ok(microDollars(value[field]) > 0n, `Invalid ${field}`);
  }
  assert.ok(value.input_token_allowance >= value.max_prompt_bytes,
    'The declared input allowance must conservatively cover the entire bounded prompt');
  assert.ok(microDollars(value.budget_usd) <= 100000000n, 'This small calibration supports at most 100 USD');
  assert.match(value.pricing_checked, /^\d{4}-\d{2}-\d{2}$/);
  assert.ok(Number.isFinite(Date.parse(value.pricing_checked)), 'Invalid pricing date');
  assert.equal(new Date(value.pricing_checked).toISOString().slice(0, 10), value.pricing_checked, 'Invalid pricing date');
  if (live) {
    const age = Date.now() - Date.parse(value.pricing_checked);
    assert.ok(age >= -86400000 && age <= 7 * 86400000, 'Recheck pricing within seven days');
    const source = new URL(value.pricing_source);
    const allowed = value.provider === 'openai'
      ? ['openai.com', 'developers.openai.com', 'platform.openai.com']
      : ['fireworks.ai', 'docs.fireworks.ai'];
    assert.ok(source.protocol === 'https:' && allowed.includes(source.hostname)
      && !source.username && !source.password, 'Use an official pricing source');
  } else assert.equal(value.model, 'fixture');
  return Object.freeze({ ...value });
}

export function loadPlan(file, live = false) {
  const bytes = readFileSync(file);
  assert.ok(bytes.length <= 8192, 'Plan is too large');
  return { plan: validatePlan(JSON.parse(bytes), live), sha256: createHash('sha256').update(bytes).digest('hex') };
}

export function readPrivateKey(file) {
  const info = statSync(file);
  assert.ok(info.isFile() && info.uid === process.getuid() && (info.mode & 0o077) === 0,
    'Provider key must be an owner-only regular file');
  assert.ok(info.size > 0 && info.size <= 16384, 'Invalid provider key file size');
  const key = readFileSync(file, 'utf8').trim();
  assert.ok(key.length > 0 && !/[\r\n]/.test(key), 'Invalid provider key file');
  return key;
}

export function scriptedPlan() {
  return { version: 1, provider: 'scripted', model: 'fixture', max_requests: 12,
    concurrency: 4, max_prompt_bytes: 65536, input_token_allowance: 131072,
    max_output_tokens: 256, timeout_seconds: 30, budget_usd: '2.00',
    input_usd_per_million: '1.00', output_usd_per_million: '2.00',
    pricing_source: 'scripted arithmetic only; no paid inference',
    pricing_checked: new Date().toISOString().slice(0, 10) };
}
