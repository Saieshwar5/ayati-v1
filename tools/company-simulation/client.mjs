import { performance } from 'node:perf_hooks';
import { randomUUID, createHash } from 'node:crypto';
import { setTimeout as delay } from 'node:timers/promises';
export { randomUUID, delay };
export const checksum = bytes => createHash('sha256').update(bytes).digest('hex');
export function clients(base, samples) {
  const sessions = new Map();
  async function request(employee, path, body, { expected = 200, label = path.replace(/[0-9a-f-]{36}/g, ':id'), raw, mime } = {}) {
    const start = performance.now();
    const response = await fetch(base + path, { method: body === undefined && raw === undefined ? 'GET' : 'POST',
      headers: { Origin: base, 'X-Ayati-Request': 'identity', ...(sessions.has(employee) ? { Cookie: sessions.get(employee) } : {}), ...((body !== undefined || raw !== undefined) ? { 'Content-Type': mime ?? 'application/json' } : {}) },
      body: raw ?? (body === undefined ? undefined : JSON.stringify(body)), signal: AbortSignal.timeout(30000) });
    const bytes = Buffer.from(await response.arrayBuffer());
    samples.push({ request_id:response.headers.get("x-relay-request-id"),server_timing:response.headers.get("server-timing"),employee, label, status: response.status, ms: performance.now() - start, bytes: bytes.length, at: Date.now() });
    if (response.headers.has('set-cookie')) sessions.set(employee, response.headers.get('set-cookie').split(';')[0]);
    if (response.status !== expected) throw new Error(`${label}: expected HTTP ${expected}, received ${response.status}: ${bytes.toString().slice(0, 400)}`);
    if (response.headers.get('content-type')?.includes('application/json')) return JSON.parse(bytes);
    return bytes;
  }
  return { request, sessions };
}
export async function until(read, predicate, label, timeout = 45000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) { const value = await read(); if (predicate(value)) return value; await delay(100); }
  throw new Error(`Timed out: ${label}`);
}
export function distribution(values) {
  const sorted = [...values].sort((a,b) => a-b), at = p => sorted[Math.min(sorted.length-1, Math.ceil(sorted.length*p)-1)] ?? null;
  return { count: sorted.length, median_ms: at(.5), p95_ms: at(.95), p99_ms: at(.99), max_ms: sorted.at(-1) ?? null };
}
