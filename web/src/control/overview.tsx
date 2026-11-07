import { useResource } from '../react/requests';
import { control, count, money, type Alert, type Overview } from './api';
import type { Go } from './app';
import { Loading, providerHue, usePolling } from './shared';

const load = () => control<Overview>('/overview');

export function OverviewScreen({ go }: { go: Go }) {
  const { data, error, refresh } = useResource(load);
  usePolling(refresh, 15_000);
  if (!data) return <Loading error={error} label="the overview" />;
  const failed = data.calls_1h ? 100 * data.errors_1h / data.calls_1h : 0;
  const slots = data.pool.ready + data.pool.busy;
  return <div className="cc-stack">
    {error && <p className="cc-alert" role="alert">Showing the last good reading. {error}</p>}
    <section className="cc-tiles" aria-label="Key numbers">
      <Tile label="Active workspaces" value={count(data.active_7d)} note={`of ${count(data.workspaces)} · used in the last 7 days`} />
      <Tile label="Model calls / min" value={data.calls_per_min.toFixed(1)} note={`${count(data.calls_1h)} in the last hour`} />
      <Tile label="Failed calls · 1 h" value={`${failed.toFixed(1)}%`} tone={failed >= 5 ? 'warn' : undefined}
        note={`${count(data.errors_1h)} failed · ${count(data.fell_back_1h)} answered by a backup`} />
      <Tile label="Executor slots" value={`${data.pool.busy}/${slots}`} tone={data.pool.oldest_queued_seconds > 120 ? 'warn' : undefined}
        note={`${data.pool.queued} waiting · ${data.pool.executors} executors`} />
      <Tile label="Charged today" value={money(data.today.charge)}
        note={`cost ${money(data.today.cost)}${data.daily_ceiling === null ? '' : ` of ${money(data.daily_ceiling)} ceiling`} · ${money(data.month.charge)} this month`} />
    </section>
    <div className="cc-row">
      <section className="cc-card cc-grow-2" aria-labelledby="cc-alerts">
        <div className="cc-card-head"><h2 id="cc-alerts">Open alerts</h2><span className="cc-dim cc-num">{data.alerts.length} open</span></div>
        {data.alerts.length === 0 && <p className="cc-empty">Nothing needs attention. Alerts appear here when a provider fails, a workspace nears its cap or work waits for executors.</p>}
        {data.alerts.map((alert, index) => <AlertRow key={`${alert.kind}-${alert.target ?? index}`} alert={alert} go={go} />)}
      </section>
      <section className="cc-card cc-grow-3" aria-labelledby="cc-traffic">
        <div className="cc-card-head"><h2 id="cc-traffic">Model calls · last 60 minutes</h2>
          <div className="cc-legend">{data.traffic.map(t => <span key={t.provider}><span className="cc-dot" style={{ background: providerHue(t.provider) }} aria-hidden="true" />{t.provider}</span>)}</div>
        </div>
        <Traffic traffic={data.traffic} />
      </section>
    </div>
    <section className="cc-card" aria-labelledby="cc-busy">
      <div className="cc-card-head"><h2 id="cc-busy">Busiest workspaces right now</h2>
        <button type="button" className="cc-btn ghost" onClick={() => go('workspaces')}>All workspaces</button></div>
      {data.busiest.length === 0 ? <p className="cc-empty">No model calls in the last 5 minutes.</p> :
        <div className="cc-scroll"><table className="cc-table">
          <thead><tr><th scope="col">Workspace</th><th scope="col">Running</th><th scope="col">Queued</th><th scope="col">Calls · 5 min</th><th scope="col">Charged today</th></tr></thead>
          <tbody>{data.busiest.map(w => <tr key={w.id}>
            <td><button type="button" className="cc-link" onClick={() => go('workspaces', { id: w.id })}>{w.name}</button></td>
            <td className="cc-num">{w.running}</td><td className="cc-num">{w.queued}</td><td className="cc-num">{w.calls_5m}</td><td className="cc-num">{money(w.charge_today)}</td>
          </tr>)}</tbody>
        </table></div>}
    </section>
  </div>;
}

function Tile({ label, value, note, tone }: { label: string; value: string; note: string; tone?: 'warn' }) {
  return <div className="cc-card cc-tile"><div className="cc-cap">{label}</div>
    <div className={`cc-tile-value cc-num${tone ? ` ${tone}` : ''}`}>{value}</div><div className="cc-dim cc-small">{note}</div></div>;
}

function AlertRow({ alert, go }: { alert: Alert; go: Go }) {
  const action = alert.kind.startsWith('provider') ? <button type="button" className="cc-btn" onClick={() => go('providers', { id: alert.target ?? '' })}>Open provider</button>
    : alert.kind === 'budget' ? <button type="button" className="cc-btn" onClick={() => go('workspaces', { id: alert.target ?? '' })}>Open workspace</button>
    : alert.kind === 'reports' ? <button type="button" className="cc-btn" onClick={() => go('reports')}>Read</button>
    : alert.kind === 'codes' ? <button type="button" className="cc-btn" onClick={() => go('codes')}>See codes</button>
    : alert.kind === 'ceiling' ? <button type="button" className="cc-btn" onClick={() => go('config')}>Configuration</button> : null;
  return <div className={`cc-alert-row ${alert.level}`}>
    <div><div className="cc-alert-title"><span className={`cc-tag ${alert.level === 'bad' ? 'bad' : alert.level === 'warn' ? 'warn' : 'new'}`}>{alert.level === 'info' ? 'INFO' : alert.level === 'bad' ? 'ACTION' : 'WARN'}</span>
      <strong>{alert.title}</strong></div><p className="cc-dim">{alert.body}</p></div>
    {action}
  </div>;
}

/** Stacked areas, one per provider, in 5-minute buckets. */
function Traffic({ traffic }: { traffic: Overview['traffic'] }) {
  if (!traffic.length) return <p className="cc-empty">No model calls in the last hour.</p>;
  const W = 600, H = 160, n = 12;
  const totals = Array.from({ length: n }, (_, i) => traffic.reduce((sum, t) => sum + (t.buckets[i] ?? 0), 0));
  const max = Math.max(1, ...totals);
  const x = (i: number) => (i * W / (n - 1)).toFixed(1);
  const y = (v: number) => (H - v / max * (H - 8)).toFixed(1);
  let below = Array(n).fill(0) as number[];
  const layers = traffic.map(t => {
    const top = below.map((b, i) => b + (t.buckets[i] ?? 0));
    const path = `M${top.map((v, i) => `${x(i)},${y(v)}`).join(' L')} L${[...below].reverse().map((v, i) => `${x(n - 1 - i)},${y(v)}`).join(' L')} Z`;
    below = top;
    return { provider: t.provider, path };
  });
  return <div className="cc-chart">
    <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" role="img"
      aria-label={`Model calls per 5 minutes over the last hour; ${traffic.map(t => `${t.provider} ${t.buckets.reduce((a, b) => a + b, 0)}`).join(', ')}; peak ${max} per 5 minutes`}>
      {[0.25, 0.5, 0.75].map(f => <line key={f} x1="0" x2={W} y1={H * f} y2={H * f} className="cc-gridline" />)}
      {layers.map(layer => <path key={layer.provider} d={layer.path} fill={providerHue(layer.provider)} fillOpacity="0.85" />)}
    </svg>
    <div className="cc-axis cc-num cc-dim"><span>60 min ago</span><span>30 min</span><span>now · peak {max} / 5 min</span></div>
  </div>;
}
