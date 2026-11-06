import { useCallback, useState } from 'react';
import { useResource } from '../requests';
import { accessRequest } from './model';
import { dollars, tokens } from '../usage/usage-model';
import '../usage/usage.css';

type Line = { key: string; calls: number; after_fallback: number; tokens: number; charge_usd: number; cost_usd: number; margin_usd: number; latency_avg_ms: number; latency_p95_ms: number };
type Price = { input: number; output: number };
type Route = { provider: string; model: string; priority: number; weight: number; cost: Price; margin: Price };
type Operations = {
  hours: number; providers: Line[]; models: Line[];
  failures: { provider: string; status: number; count: number; last_ms: number }[];
  resting: { provider: string; seconds: number }[];
  catalog: { on: boolean; error?: string; models: { id: string; name: string; status: string; price: Price; routes: Route[] }[] };
};
const windows: [number, string][] = [[24, 'Last 24 hours'], [168, 'Last 7 days'], [720, 'Last 30 days']];
const perMillion = (p: Price) => `${dollars(p.input)} / ${dollars(p.output)}`;
const status = (n: number) => n === 0 ? 'Unreachable' : n === 429 ? '429 rate limited' : String(n);

/** The operator view (MG4), for Cuckoo's platform administrators: provider traffic,
 * fallbacks, failures and speed, what people were charged against what providers
 * charged us, and the catalog's routes with their margins. */
export function ModelOperations() {
  const [hours, setHours] = useState(24);
  const load = useCallback(() => accessRequest<Operations>(`/admin/models?hours=${hours}`), [hours]);
  const resource = useResource(load);
  const data = resource.data?.hours === hours ? resource.data : undefined;
  return <section className="access-box model-operations" aria-label="Models and providers">
    <div className="usage-report-head"><h2>Models and providers</h2>
      <label className="usage-period">Period<select value={hours} onChange={event => setHours(Number(event.target.value))}>
        {windows.map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label></div>
    {resource.error && <p role="alert">{resource.error}</p>}
    {!data && !resource.error && <p role="status">Loading model operations…</p>}
    {data && <>
      {data.resting.length > 0 && <p role="status" className="model-operations-resting">Resting after a failure on this controller: {data.resting.map(r => `${r.provider} (${r.seconds}s)`).join(', ')}.</p>}
      {data.catalog.error && <p role="alert">The model catalog can't be read: {data.catalog.error}</p>}
      <Lines caption="Providers" first="Provider" rows={data.providers} />
      <div className="usage-table-wrap"><table className="usage-table"><caption>Failures</caption>
        <thead><tr><th scope="col">Provider</th><th scope="col">Answer</th><th scope="col">Times</th><th scope="col">Last</th></tr></thead>
        <tbody>{data.failures.length ? data.failures.map(f => <tr key={`${f.provider}/${f.status}`}><th scope="row">{f.provider}</th><td>{status(f.status)}</td><td>{tokens(f.count)}</td>
          <td>{new Date(f.last_ms).toLocaleString()}</td></tr>) : <tr><td colSpan={4}>No failures.</td></tr>}</tbody></table></div>
      <Lines caption="Models" first="Model" rows={data.models} />
      {data.catalog.on && <div className="usage-table-wrap"><table className="usage-table"><caption>Catalog routes (dollars per million tokens, in / out)</caption>
        <thead><tr><th scope="col">Model</th><th scope="col">Provider</th><th scope="col">Their name</th><th scope="col">Priority</th><th scope="col">Weight</th><th scope="col">Price</th><th scope="col">Cost</th><th scope="col">Margin</th></tr></thead>
        <tbody>{data.catalog.models.flatMap(m => m.routes.map(r => <tr key={`${m.id}/${r.provider}/${r.model}`}>
          <th scope="row">{m.name}{m.status !== 'available' ? ` · ${m.status}` : ''}</th><td>{r.provider}</td><td>{r.model}</td><td>{r.priority}</td><td>{r.weight}</td>
          <td>{perMillion(m.price)}</td><td>{perMillion(r.cost)}</td><td className={r.margin.input < 0 || r.margin.output < 0 ? 'model-operations-loss' : undefined}>{perMillion(r.margin)}</td>
        </tr>))}</tbody></table></div>}
    </>}
  </section>;
}

function Lines({ caption, first, rows }: { caption: string; first: string; rows: Line[] }) {
  return <div className="usage-table-wrap"><table className="usage-table"><caption>{caption}</caption>
    <thead><tr><th scope="col">{first}</th><th scope="col">Calls</th><th scope="col">After a fallback</th><th scope="col">Tokens</th><th scope="col">Average</th><th scope="col">95th percentile</th>
      <th scope="col">Charged</th><th scope="col">Cost</th><th scope="col">Margin</th></tr></thead>
    <tbody>{rows.length ? rows.map(row => <tr key={row.key}><th scope="row">{row.key}</th><td>{tokens(row.calls)}</td><td>{tokens(row.after_fallback)}</td><td>{tokens(row.tokens)}</td>
      <td>{tokens(row.latency_avg_ms)} ms</td><td>{tokens(row.latency_p95_ms)} ms</td><td>{dollars(row.charge_usd)}</td><td>{dollars(row.cost_usd)}</td>
      <td className={row.margin_usd < 0 ? 'model-operations-loss' : undefined}>{dollars(row.margin_usd)}</td></tr>)
      : <tr><td colSpan={9}>No calls in this period.</td></tr>}</tbody></table></div>;
}
