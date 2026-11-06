import { useCallback, useState } from 'react';
import { useResource } from '../react/requests';
import { control, count, money, type Spend } from './api';
import type { Go } from './app';
import { CapMeter } from './workspaces';
import { Loading, providerHue } from './shared';

export function SpendScreen({ go }: { go: Go }) {
  const [days, setDays] = useState(30);
  const load = useCallback(() => control<Spend>(`/spend?days=${days}`), [days]);
  const { data, error } = useResource(load);
  if (!data) return <Loading error={error} label="spend" />;
  const unpriced = data.by_model.reduce((sum, m) => sum + m.unpriced_calls, 0);
  const peak = Math.max(0.01, ...data.daily.map(d => d.cost));
  return <div className="cc-stack">
    <div className="cc-toolbar" role="group" aria-label="Period">
      {[7, 30, 90].map(n => <button key={n} type="button" className="cc-chip" aria-pressed={days === n} onClick={() => setDays(n)}>Last {n} days</button>)}
    </div>
    <section className="cc-tiles" aria-label="Spend numbers">
      <div className="cc-card cc-tile"><div className="cc-cap">This month</div><div className="cc-tile-value cc-num">{money(data.month)}</div><div className="cc-dim cc-small">since the 1st (UTC)</div></div>
      <div className="cc-card cc-tile"><div className="cc-cap">Forecast</div><div className="cc-tile-value cc-num">{money(data.forecast)}</div><div className="cc-dim cc-small">at this month’s pace</div></div>
      <div className="cc-card cc-tile"><div className="cc-cap">Today</div><div className="cc-tile-value cc-num">{money(data.today)}</div>
        <div className="cc-dim cc-small">{data.daily_ceiling === null ? 'no daily ceiling' : `ceiling ${money(data.daily_ceiling)}`}</div></div>
      <div className="cc-card cc-tile"><div className="cc-cap">Default cap</div><div className="cc-tile-value cc-num">{data.default_cap === null ? '—' : money(data.default_cap, 0)}</div>
        <div className="cc-dim cc-small"><button type="button" className="cc-link" onClick={() => go('config')}>Change in Configuration</button></div></div>
    </section>
    {unpriced > 0 && <p className="cc-alert" role="status">{count(unpriced)} answered call(s) this month have no price, so totals are low. Add their models with prices in Providers.</p>}
    <section className="cc-card" aria-labelledby="cc-daily">
      <div className="cc-card-head"><h2 id="cc-daily">Spend per day</h2><span className="cc-dim cc-small">peak {money(peak)}</span></div>
      <div className="cc-bars" role="img" aria-label={`Spend per day over ${data.daily.length} days; peak ${money(peak)}; today ${money(data.daily.at(-1)?.cost ?? 0)}`}>
        {data.daily.map(d => <span key={d.day_ms} title={`${new Date(d.day_ms).toLocaleDateString()} · ${money(d.cost)} · ${d.calls} calls`}
          style={{ height: `${Math.max(2, 100 * d.cost / peak)}%` }} />)}
      </div>
    </section>
    <div className="cc-row cc-top-align">
      <section className="cc-card cc-grow-1 cc-scroll" aria-labelledby="cc-by-model">
        <div className="cc-card-head"><h2 id="cc-by-model">By model · this month</h2></div>
        {!data.by_model.length ? <p className="cc-empty">No model calls this month.</p> : <table className="cc-table">
          <thead><tr><th scope="col">Model</th><th scope="col">Calls</th><th scope="col">Tokens in / out</th><th scope="col">Cost</th></tr></thead>
          <tbody>{data.by_model.map(m => <tr key={`${m.provider}:${m.model}`}>
            <td><span className="cc-dot" style={{ background: providerHue(m.provider) }} aria-hidden="true" /> <span className="cc-mono">{m.provider}:{m.model}</span>
              {m.unpriced_calls > 0 && <span className="cc-tag warn">UNPRICED</span>}</td>
            <td className="cc-num">{count(m.calls)}</td><td className="cc-num">{count(m.input_tokens)} / {count(m.output_tokens)}</td><td className="cc-num">{money(m.cost)}</td>
          </tr>)}</tbody></table>}
      </section>
      <section className="cc-card cc-grow-1 cc-scroll" aria-labelledby="cc-by-ws">
        <div className="cc-card-head"><h2 id="cc-by-ws">By workspace · this month</h2></div>
        {!data.by_workspace.length ? <p className="cc-empty">No workspace spent anything yet.</p> : <table className="cc-table">
          <thead><tr><th scope="col">Workspace</th><th scope="col">Spend / cap</th><th scope="col">Biggest agent</th></tr></thead>
          <tbody>{data.by_workspace.map(w => <tr key={w.id}>
            <td><button type="button" className="cc-link" onClick={() => go('workspaces', { id: w.id })}>{w.name}</button></td>
            <td><CapMeter spend={w.cost} cap={w.cap} /></td>
            <td className="cc-small">{w.top_agent ? `${w.top_agent.name} · ${money(w.top_agent.cost)}` : '—'}</td>
          </tr>)}</tbody></table>}
      </section>
    </div>
  </div>;
}
