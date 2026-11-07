import { useCallback, useState } from 'react';
import { useResource } from '../react/requests';
import { control, count, money, type Spend } from './api';
import type { Go } from './app';
import { BudgetMeter } from './workspaces';
import { Loading } from './shared';

/** Charged is what people pay at catalog prices; cost is what providers charge us. */
export function SpendScreen({ go }: { go: Go }) {
  const [days, setDays] = useState(30);
  const load = useCallback(() => control<Spend>(`/spend?days=${days}`), [days]);
  const { data, error } = useResource(load);
  if (!data) return <Loading error={error} label="spend" />;
  const peak = Math.max(0.01, ...data.daily.map(d => Math.max(d.charge, d.cost)));
  const loss = data.month.margin < 0;
  return <div className="cc-stack">
    <div className="cc-toolbar" role="group" aria-label="Period">
      {[7, 30, 90].map(n => <button key={n} type="button" className="cc-chip" aria-pressed={days === n} onClick={() => setDays(n)}>Last {n} days</button>)}
    </div>
    <section className="cc-tiles" aria-label="Spend numbers">
      <div className="cc-card cc-tile"><div className="cc-cap">Charged this month</div><div className="cc-tile-value cc-num">{money(data.month.charge)}</div>
        <div className="cc-dim cc-small">forecast {money(data.forecast.charge)}</div></div>
      <div className="cc-card cc-tile"><div className="cc-cap">Provider cost this month</div><div className="cc-tile-value cc-num">{money(data.month.cost)}</div>
        <div className="cc-dim cc-small">forecast {money(data.forecast.cost)}</div></div>
      <div className="cc-card cc-tile"><div className="cc-cap">Margin this month</div><div className={`cc-tile-value cc-num${loss ? ' bad' : ''}`}>{money(data.month.margin)}</div>
        <div className="cc-dim cc-small">charged minus cost</div></div>
      <div className="cc-card cc-tile"><div className="cc-cap">Provider cost today</div><div className="cc-tile-value cc-num">{money(data.today.cost)}</div>
        <div className="cc-dim cc-small">{data.daily_ceiling === null ? <>no daily ceiling · <button type="button" className="cc-link" onClick={() => go('config')}>set one</button></> : `ceiling ${money(data.daily_ceiling)}`}</div></div>
    </section>
    <section className="cc-card" aria-labelledby="cc-daily">
      <div className="cc-card-head"><h2 id="cc-daily">Per day</h2>
        <div className="cc-legend"><span><span className="cc-dot cc-charge" aria-hidden="true" />Charged</span><span><span className="cc-dot cc-cost" aria-hidden="true" />Cost</span></div></div>
      <div className="cc-bars cc-bars-pair" role="img" aria-label={`Charged and cost per day over ${data.daily.length} days; peak ${money(peak)}`}>
        {data.daily.map(d => <span key={d.day_ms} className="cc-bar-pair" title={`${new Date(d.day_ms).toLocaleDateString()} · charged ${money(d.charge)} · cost ${money(d.cost)} · ${d.calls} calls`}>
          <span className="cc-charge" style={{ height: `${Math.max(1, 100 * d.charge / peak)}%` }} />
          <span className="cc-cost" style={{ height: `${Math.max(1, 100 * d.cost / peak)}%` }} />
        </span>)}
      </div>
    </section>
    <div className="cc-row cc-top-align">
      <section className="cc-card cc-grow-1 cc-scroll" aria-labelledby="cc-by-model">
        <div className="cc-card-head"><h2 id="cc-by-model">By model · this month</h2></div>
        {!data.by_model.length ? <p className="cc-empty">No model calls this month.</p> : <table className="cc-table">
          <thead><tr><th scope="col">Model</th><th scope="col">Calls</th><th scope="col">Charged</th><th scope="col">Cost</th><th scope="col">Margin</th></tr></thead>
          <tbody>{data.by_model.map(m => <tr key={m.model}><td className="cc-mono">{m.model}</td><td className="cc-num">{count(m.calls)}</td>
            <td className="cc-num">{money(m.charge)}</td><td className="cc-num">{money(m.cost)}</td><td className={`cc-num${m.margin < 0 ? ' bad' : ''}`}>{money(m.margin)}</td></tr>)}</tbody></table>}
      </section>
      <section className="cc-card cc-grow-1 cc-scroll" aria-labelledby="cc-by-ws">
        <div className="cc-card-head"><h2 id="cc-by-ws">By workspace · this month</h2></div>
        {!data.by_workspace.length ? <p className="cc-empty">No workspace used models yet.</p> : <table className="cc-table">
          <thead><tr><th scope="col">Workspace</th><th scope="col">Charged / budget</th><th scope="col">Margin</th><th scope="col">Biggest agent</th></tr></thead>
          <tbody>{data.by_workspace.map(w => <tr key={w.id}>
            <td><button type="button" className="cc-link" onClick={() => go('workspaces', { id: w.id })}>{w.name}</button></td>
            <td><BudgetMeter charge={w.charge} budget={w.budget} /></td>
            <td className={`cc-num${w.margin < 0 ? ' bad' : ''}`}>{money(w.margin)}</td>
            <td className="cc-small">{w.top_agent ? `${w.top_agent.name} · ${money(w.top_agent.charge)}` : '—'}</td>
          </tr>)}</tbody></table>}
      </section>
    </div>
  </div>;
}
