import assert from 'node:assert/strict';
import { microDollars } from './plan.mjs';

const charge = (tokens, rate) => (BigInt(tokens) * rate + 999999n) / 1000000n;
export class Budget {
  constructor(plan) {
    this.plan = plan;
    this.inputRate = microDollars(plan.input_usd_per_million);
    this.outputRate = microDollars(plan.output_usd_per_million);
    this.limit = microDollars(plan.budget_usd);
    this.perCall = this.cost(plan.input_token_allowance, plan.max_output_tokens);
    this.reserved = 0n;
    this.active = 0;
    this.rows = [];
    this.denied = [];
    this.fenced = false;
  }
  cost(input, output) { return charge(input, this.inputRate) + charge(output, this.outputRate); }
  reserve(marker, promptBytes) {
    // Synchronous reservation precedes the first await/network action. Never
    // refund: retries, cancellation and missing usage all retain their allowance.
    const reason = this.fenced ? 'usage_exceeded_allowance'
      : this.rows.length >= this.plan.max_requests ? 'request_limit'
      : this.active >= this.plan.concurrency ? 'concurrency_limit'
      : this.reserved + this.perCall > this.limit ? 'budget_limit' : null;
    if (reason) {
      if (this.denied.length < 100) this.denied.push({ marker, reason, at_ms: Date.now() });
      throw new Error(reason);
    }
    this.reserved += this.perCall;
    this.active++;
    const row = { index: this.rows.length, marker, prompt_bytes: promptBytes,
      reserved_micro_usd: this.perCall.toString(), state: 'reserved', usage: null };
    this.rows.push(row);
    return row;
  }
  usage(row, value) {
    const input = value.prompt_tokens, output = value.completion_tokens;
    assert.ok(Number.isSafeInteger(input) && input >= 0 && Number.isSafeInteger(output) && output >= 0,
      'Invalid usage counters');
    assert.equal(row.usage, null, 'Duplicate usage');
    row.usage = { prompt_tokens: input, completion_tokens: output };
    row.usage_cost_estimate_micro_usd = this.cost(input, output).toString();
    if (input > this.plan.input_token_allowance || output > this.plan.max_output_tokens) {
      this.fenced = true;
      throw new Error('usage_exceeded_allowance');
    }
  }
  finish(row, state) {
    assert.ok(!row.finished_ms, 'Request already finished');
    row.state = state;
    row.finished_ms = Date.now();
    this.active--;
  }
  report() {
    return { reserved_micro_usd: this.reserved.toString(), limit_micro_usd: this.limit.toString(),
      usage_cost_estimate_micro_usd: this.rows.reduce((sum, row) => sum + BigInt(row.usage_cost_estimate_micro_usd ?? 0), 0n).toString(),
      unknown_usage_requests: this.rows.filter(row => !row.usage).length,
      active: this.active, fenced: this.fenced, requests: this.rows, denied: this.denied,
      accounting: 'Declared input allowance plus bounded output, reserved before every forwarded call; no refunds. Rates/allowance need operator verification; this is not a provider billing cap or invoice.' };
  }
}
