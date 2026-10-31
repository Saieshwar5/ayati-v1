import { AgentFace } from '../faces';
import { startOfToday, type Totals } from './model';

/** Local calendar days from `from` to today, as the server's per-day keys. */
function days(from: number): string[] {
  const out: string[] = [];
  const day = new Date(from); day.setHours(0, 0, 0, 0);
  const today = startOfToday();
  while (day.getTime() <= today && out.length < 400) {
    out.push(`${day.getFullYear()}-${String(day.getMonth() + 1).padStart(2, '0')}-${String(day.getDate()).padStart(2, '0')}`);
    day.setDate(day.getDate() + 1);
  }
  return out;
}
const label = (key: string, style: 'short' | 'long') => {
  const [y, m, d] = key.split('-').map(Number);
  const date = new Date(y, m - 1, d);
  return style === 'short' && days(startOfToday() - 6 * 86400000).includes(key)
    ? date.toLocaleDateString([], { weekday: 'short' })
    : date.toLocaleDateString([], { day: 'numeric', month: 'short', ...(style === 'long' ? { weekday: 'short' } : {}) });
};
const requests = (n: number) => `${n.toLocaleString()} ${n === 1 ? 'request' : 'requests'}`;

/** Requests per day and by agent for History's period (designs/activity 3).
 * One series each, so the titles name them; every value is also in the table. */
export function ActivityCharts({ totals, from }: { totals: Totals; from: number }) {
  const counts = new Map(totals.per_day.map(day => [day.day, day.count]));
  const series = days(from).map(day => ({ day, count: counts.get(day) ?? 0 }));
  const peak = Math.max(1, ...series.map(day => day.count));
  const peakDay = series.find(day => day.count === peak)?.day;
  // About seven date labels, whatever the period.
  const every = Math.max(1, Math.ceil(series.length / 7));
  const top = Math.max(1, ...totals.by_agent.map(agent => agent.count));
  return <div className="activity-charts">
    <figure className="activity-chart" aria-labelledby="chart-per-day">
      <figcaption id="chart-per-day">Requests per day</figcaption>
      <ol className="chart-columns" style={{ ['--columns' as string]: series.length }}>{series.map((day, index) =>
        <li key={day.day} tabIndex={0} aria-label={`${label(day.day, 'long')}: ${requests(day.count)}`}>
          <span className="chart-column-slot">
            <span className="chart-column" style={{ height: `${(day.count / peak) * 100}%` }}>
              {day.day === peakDay && day.count > 0 && <span className="chart-value" aria-hidden="true">{day.count}</span>}
            </span>
          </span>
          <span className="chart-axis-label" aria-hidden="true">{index % every === 0 ? label(day.day, 'short') : ''}</span>
          <span className="chart-tip" role="tooltip"><strong>{day.count}</strong> {day.count === 1 ? 'request' : 'requests'} · {label(day.day, 'long')}</span>
        </li>)}</ol>
    </figure>
    <figure className="activity-chart" aria-labelledby="chart-by-agent">
      <figcaption id="chart-by-agent">Requests by agent</figcaption>
      {!totals.by_agent.length && <p className="activity-empty">No requests in this period.</p>}
      <ul className="chart-bars">{totals.by_agent.map(agent => <li key={agent.agent} tabIndex={0} aria-label={`${agent.name}: ${requests(agent.count)}`}>
        <span className="chart-bar-name"><AgentFace agent={{ id: agent.agent, name: agent.name, avatar: agent.avatar }} className="activity-face" />{agent.name}</span>
        <span className="chart-bar-track"><span className="chart-bar" style={{ width: `${(agent.count / top) * 100}%` }} /></span>
        <span className="chart-bar-value">{agent.count}</span>
      </li>)}</ul>
    </figure>
    <details className="chart-table"><summary>Show as table</summary>
      <table><caption>Requests per day</caption><thead><tr><th scope="col">Day</th><th scope="col">Requests</th></tr></thead>
        <tbody>{series.map(day => <tr key={day.day}><th scope="row">{label(day.day, 'long')}</th><td>{day.count}</td></tr>)}</tbody></table>
      <table><caption>Requests by agent</caption><thead><tr><th scope="col">Agent</th><th scope="col">Requests</th></tr></thead>
        <tbody>{totals.by_agent.map(agent => <tr key={agent.agent}><th scope="row">{agent.name}</th><td>{agent.count}</td></tr>)}</tbody></table>
    </details>
  </div>;
}
