import { useCallback, useState } from 'react';
import { useResource } from '../requests';
import { dollars, loadUsage, periods, tokens, type ComputerUsage, type UsageReport, type UsageRow } from './usage-model';
import { workingTime } from '../agents/computer/model';
import './usage.css';

/** Model use over a chosen period (MG2): totals, then by agent, model, person and day.
 * With `agent`, one agent's use; otherwise whatever the viewer may see. */
export function UsageReportView({ agent, title }: { agent?: string; title: string }) {
  const [days, setDays] = useState(30);
  // A new period or agent starts a fresh read; an older answer never replaces it.
  const load = useCallback(() => loadUsage(days, agent), [days, agent]);
  const report = useResource(load);
  const data = report.data;
  const current = data && data.days === days ? data : undefined;
  return <section className="usage-report" aria-label={title}>
    <header className="usage-report-head">
      <h3>{title}</h3>
      <label className="usage-period">Period
        <select value={days} onChange={event => setDays(Number(event.target.value))}>
          {periods.map(n => <option key={n} value={n}>Last {n} days</option>)}
        </select>
      </label>
    </header>
    {report.error && <p role="alert">{report.error}</p>}
    {!current && !report.error && <p role="status">Loading usage…</p>}
    {current && <UsageBody report={current} agent={Boolean(agent)} />}
  </section>;
}

function UsageBody({ report, agent }: { report: UsageReport; agent: boolean }) {
  const { totals } = report;
  const computers = report.computers?.seconds ? report.computers : undefined;
  if (!totals.calls && !computers) return <p className="ui-help">No model or computer use in the last {report.days} days.</p>;
  return <>
    {report.scope === 'mine' && <p className="ui-help">The model and computer use made for your requests.</p>}
    <div className="usage-totals">
      <article aria-label="Charges"><small>Charges</small><strong>{dollars(report.total_charge_usd ?? totals.charge_usd)}</strong></article>
      {computers && <article aria-label="Computer time"><small>Computer time</small><strong>{workingTime(computers.seconds)}</strong></article>}
      <article aria-label="Model calls"><small>Model calls</small><strong>{tokens(totals.calls)}</strong></article>
      <article aria-label="Tokens read"><small>Tokens read</small><strong>{tokens(totals.input_tokens)}</strong></article>
      <article aria-label="Tokens written"><small>Tokens written</small><strong>{tokens(totals.output_tokens)}</strong></article>
    </div>
    <div className="usage-tables">
      {computers && !agent && <ComputerTable usage={computers} />}
      {!agent && <UsageTable caption="By agent" first="Agent" rows={report.by_agent} />}
      <UsageTable caption="By model" first="Model" rows={report.by_model} />
      {report.scope !== 'mine' && <UsageTable caption="By person" first="Person" rows={report.by_person} />}
      <UsageTable caption="By day" first="Day" rows={report.by_day} />
    </div>
  </>;
}

function UsageTable({ caption, first, rows }: { caption: string; first: string; rows: UsageRow[] }) {
  return <div className="usage-table-wrap"><table className="usage-table">
    <caption>{caption}</caption>
    <thead><tr><th scope="col">{first}</th><th scope="col">Calls</th><th scope="col">Tokens read</th><th scope="col">Tokens written</th><th scope="col">Charges</th></tr></thead>
    <tbody>{rows.map(row => <tr key={row.key || '-'}>
      <th scope="row">{row.name ?? row.key}</th><td>{tokens(row.calls)}</td><td>{tokens(row.input_tokens)}</td><td>{tokens(row.output_tokens)}</td><td>{dollars(row.charge_usd)}</td>
    </tr>)}</tbody>
  </table></div>;
}

/** Computer time by agent: how long each agent's computer was awake, and its charge. */
function ComputerTable({ usage }: { usage: ComputerUsage }) {
  return <div className="usage-table-wrap"><table className="usage-table">
    <caption>Computers by agent</caption>
    <thead><tr><th scope="col">Agent</th><th scope="col">Working time</th><th scope="col">Charges</th></tr></thead>
    <tbody>{usage.by_agent.map(row => <tr key={row.key}>
      <th scope="row">{row.name ?? row.key}</th><td>{workingTime(row.seconds)}</td><td>{dollars(row.charge_usd)}</td>
    </tr>)}</tbody>
  </table></div>;
}
