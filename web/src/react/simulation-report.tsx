import { useCallback } from 'react';
import { identityRequest } from '../workspace-context';
import { useResource } from './requests';
import { Button } from './ui';
type Distribution = { median_ms?: number; p95_ms?: number; p99_ms?: number; count?: number };
type Bundle = {
  rehearsal: { run: string; totals: { passed: number; failed: number; http_requests: number }; latency: { http: Distribution }; agent_burst?: { offered: number; accepted: number; rejected: number; elapsed_ms: number }; checks: { name: string; status: string; error?: string }[]; limitations: { title: string; actual: string }[] };
  load?: { accepted: number; missing: number; errors: unknown[]; failed_replies: number; duration_ms: number; latency: Distribution; memory: { rss_kib: number }[] };
  browser?: { results: { name: string; passed: boolean; samples_ms?: number[]; error?: string }[] };
  retention?: { passed: boolean; messages: number; objects: number };
  recovery?: { passed: boolean; description: string };
};
const ms = (value?: number) => value === undefined ? 'Not measured' : `${Math.round(value)} ms`;
export function SimulationReport() {
  const resource = useResource(useCallback(() => identityRequest<{ reports: Bundle[] }>('/simulation/reports'), []));
  return <section><header><h2>Simulation results</h2><Button onClick={() => void resource.refresh().catch(() => {})}>Refresh results</Button></header>
    <p>Measured on this local machine with scripted agents. API timing, browser visibility and business correctness are separate checks.</p>
    {resource.error && <p role="alert">{resource.error}</p>}
    {!resource.error && resource.data?.reports.map(({ rehearsal: r, load, browser, retention, recovery }) => <details key={r.run} className="company-run"><summary>{r.run} · {r.totals.passed} passed · {r.totals.failed} failed</summary>
      <p>{r.totals.http_requests} requests · median {ms(r.latency.http.median_ms)} · p95 {ms(r.latency.http.p95_ms)} · p99 {ms(r.latency.http.p99_ms)}</p>
      {r.agent_burst && <p>Agent burst: {r.agent_burst.accepted}/{r.agent_burst.offered} accepted, {r.agent_burst.rejected} rejected; {ms(r.agent_burst.elapsed_ms)} to drain.</p>}
      {load && <p>Sustained run: {load.accepted} messages over {Math.round(load.duration_ms / 1000)} seconds; {load.missing} missing; {load.errors.length} send errors; {load.failed_replies} failed replies. Send p95: {ms(load.latency.p95_ms)}. Memory: {Math.round((load.memory[0]?.rss_kib ?? 0)/1024)} → {Math.round((load.memory.at(-1)?.rss_kib ?? 0)/1024)} MiB.</p>}
      {retention && <p>Restart retention: {retention.passed ? 'passed' : 'failed'}; {retention.messages} messages, {retention.objects} objects.</p>}
      {recovery && <p>Worker recovery: {recovery.passed ? 'passed' : 'failed'} · {recovery.description}</p>}
      <ul>{r.checks.map(c => <li key={c.name}>{c.status}: {c.name}{c.error && <pre>{c.error}</pre>}</li>)}</ul>
      {browser && <><h3>Browser checks</h3><ul>{browser.results.map(c => <li key={c.name}>{c.passed ? 'passed' : 'failed'}: {c.name}{c.samples_ms && ` · visibility ${c.samples_ms.map(n => ms(n)).join(', ')}`}{c.error}</li>)}</ul></>}
      <h3>Limits and remaining work</h3><ul>{r.limitations.map(c => <li key={c.title}><strong>{c.title}</strong>: {c.actual}</li>)}</ul>
    </details>)}
  </section>;
}
