import { useCallback, useState } from 'react';
import { useResource } from '../react/requests';
import { ago, control, count, money, type Operations, type Providers } from './api';
import type { Go } from './app';
import { Loading, Tag, providerHue } from './shared';

/** The model catalog's routes (runtime/model-catalog.md) as they are now: which
 * providers serve each model, in what order, at what cost and margin, and which
 * of them are switched off or resting. The catalog stays a reviewed file. */
export function RoutingScreen({ providers, go }: { providers?: Providers; go: Go }) {
  const [hours, setHours] = useState(24);
  const load = useCallback(() => control<Operations>(`/models?hours=${hours}`), [hours]);
  const { data, error } = useResource(load);
  if (!data) return <Loading error={error} label="the model catalog" />;
  const off = (name: string) => providers?.providers.some(p => p.name === name && !p.enabled) ?? false;
  const resting = (name: string) => data.resting.find(r => r.provider === name);
  const usage = (model: string) => data.models.find(m => m.key === model);
  return <div className="cc-stack">
    <p className="cc-dim cc-lead">People choose models; the catalog decides which providers serve them. Routes are tried by priority, and
      share traffic by weight within one. A provider that fails rests for 30 seconds, and the next route gets the request. The catalog is a
      reviewed file (<code>AYATI_MODEL_CATALOG_FILE</code>); here you see it live, and you can switch a provider off in <button type="button"
      className="cc-link" onClick={() => go('providers')}>Providers</button>.</p>
    <div className="cc-toolbar" role="group" aria-label="Period">
      {[1, 24, 168].map(n => <button key={n} type="button" className="cc-chip" aria-pressed={hours === n} onClick={() => setHours(n)}>
        {n === 1 ? 'Last hour' : n === 24 ? 'Last 24 hours' : 'Last 7 days'}</button>)}
    </div>
    {!data.catalog.on && <p className="cc-empty cc-card">No model catalog is set up, so agents name a provider and model themselves.</p>}
    {data.catalog.error && <p className="cc-alert" role="alert">The catalog file can’t be read: {data.catalog.error}</p>}
    {data.catalog.models.map(model => {
      const used = usage(model.id);
      const top = Math.min(...model.routes.map(r => r.priority));
      const live = model.routes.filter(r => !off(r.provider));
      return <section key={model.id} className="cc-card" aria-labelledby={`cc-route-${model.id}`}>
        <div className="cc-card-head">
          <div><h2 id={`cc-route-${model.id}`}>{model.name} <span className="cc-mono cc-dim">{model.id}</span></h2>
            <span className="cc-dim cc-small">Charged {money(model.price.input)} in / {money(model.price.output)} out per 1M tokens</span></div>
          <span className="cc-buttons">
            {model.status !== 'available' && <Tag tone={model.status === 'retiring' ? 'warn' : 'new'}>{model.status.toUpperCase()}</Tag>}
            {live.length === 0 && <Tag tone="bad">NO PROVIDER ON</Tag>}
            <span className="cc-num cc-small">{used ? `${count(used.calls)} calls · ${count(used.after_fallback)} after fallback · p95 ${(used.latency_p95_ms / 1000).toFixed(1)} s` : 'no calls'}</span>
          </span>
        </div>
        <div className="cc-scroll"><table className="cc-table">
          <thead><tr><th scope="col">Provider</th><th scope="col">Its model</th><th scope="col">Priority</th><th scope="col">Weight</th><th scope="col">Cost per 1M</th><th scope="col">Margin per 1M</th><th scope="col">Now</th></tr></thead>
          <tbody>{model.routes.map(route => {
            const rest = resting(route.provider);
            const loss = route.margin.input < 0 || route.margin.output < 0;
            return <tr key={`${route.provider}:${route.model}`}>
              <td><span className="cc-dot" style={{ background: providerHue(route.provider) }} aria-hidden="true" /> {route.provider}</td>
              <td className="cc-mono">{route.model}</td>
              <td className="cc-num">{route.priority}{route.priority === top ? ' · first' : ''}</td>
              <td className="cc-num">{route.weight}</td>
              <td className="cc-num">{money(route.cost.input)} / {money(route.cost.output)}</td>
              <td className={`cc-num${loss ? ' bad' : ''}`}>{money(route.margin.input)} / {money(route.margin.output)}</td>
              <td>{off(route.provider) ? <Tag tone="bad">SWITCHED OFF</Tag> : rest ? <Tag tone="warn">RESTING {rest.seconds}s</Tag> : <Tag tone="ok">ON</Tag>}</td>
            </tr>;
          })}</tbody>
        </table></div>
      </section>;
    })}
    {data.failures.length > 0 && <section className="cc-card" aria-labelledby="cc-failures">
      <div className="cc-card-head"><h2 id="cc-failures">Provider failures</h2></div>
      <table className="cc-table"><thead><tr><th scope="col">Provider</th><th scope="col">Answer</th><th scope="col">Times</th><th scope="col">Last</th></tr></thead>
        <tbody>{data.failures.map(f => <tr key={`${f.provider}-${f.status}`}><td>{f.provider}</td><td className="cc-num">{f.status === 0 ? 'Unreachable' : f.status}</td>
          <td className="cc-num">{count(f.count)}</td><td className="cc-dim">{ago(f.last_ms)}</td></tr>)}</tbody></table>
    </section>}
  </div>;
}
