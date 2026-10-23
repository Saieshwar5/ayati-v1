import assert from 'node:assert/strict';

// Preserve UTF-8 across chunks and parse bounded SSE frames. Only timing and
// numeric usage leave this observer; prompt/output/reasoning text is not retained.
export class StreamObserver {
  decoder = new TextDecoder('utf-8', { fatal: true });
  buffer = '';
  bytes = 0;
  done = false;
  constructor(row, budget) { this.row = row; this.budget = budget; }
  push(chunk) {
    this.bytes += chunk.length;
    assert.ok(this.bytes <= 1048576, 'Response exceeds calibration limit');
    this.buffer += this.decoder.decode(chunk, { stream: true });
    this.buffer = this.buffer.replaceAll('\r\n', '\n');
    const accepted = [];
    let end;
    while ((end = this.buffer.indexOf('\n\n')) !== -1) {
      const frame = this.buffer.slice(0, end);
      this.buffer = this.buffer.slice(end + 2);
      assert.ok(Buffer.byteLength(frame) <= 65536, 'SSE frame exceeds limit');
      this.frame(frame);
      accepted.push(frame + '\n\n');
    }
    assert.ok(Buffer.byteLength(this.buffer) <= 65536, 'SSE frame exceeds limit');
    return accepted;
  }
  frame(frame) {
    const data = frame.split('\n').filter(line => line.startsWith('data:')).map(line => line.slice(5).trimStart()).join('\n');
    if (!data) return;
    assert.ok(!this.done, 'Data after stream completion');
    if (data === '[DONE]') { this.done = true; return; }
    const value = JSON.parse(data);
    assert.ok(!value.error, 'Provider stream failed');
    if (typeof value.model === 'string') this.row.response_model = value.model.slice(0, 200);
    for (const choice of value.choices ?? []) {
      assert.ok(!choice.delta?.tool_calls?.length, 'Calibration is tool-free');
      if (typeof choice.delta?.content === 'string' && choice.delta.content.length) {
        this.row.first_text_ms ??= Date.now();
      }
      if (choice.finish_reason) this.row.finish_reason = String(choice.finish_reason).slice(0, 32);
    }
    if (value.usage) this.budget.usage(this.row, value.usage);
  }
  finish() {
    this.buffer += this.decoder.decode();
    assert.ok(!this.buffer.trim() && this.done, 'Incomplete provider stream');
  }
}
