import { useCallback, useState } from 'react';
import { useResource } from '../requests';
import { dollars, loadUsage, periods, tokens, type ComputerUsage, type UsageReport, type UsageRow } from './usage-model';
import { workingTime } from '../agents/computer/model';
import { UsageChart } from './usage-chart';
import './usage.css';

/** Model use over a chosen period (MG2): the charge, a bar per day, then one table by agent, model, person or day.
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

type Group = 'agent' | 'model' | 'person' | 'day' | 'computers';
const groupNames: Record<Group, [string, string]> = {
  agent: ['By agent', 'Agent'], model: ['By model', 'Model'], person: ['By person', 'Asked by'], day: ['By day', 'Day'], computers: ['Computers', 'Agent'],
};
const share = (part: number, whole: number) => whole > 0 ? `${Math.round(part / whole * 100)}%` : '0%';

/** The charge first, then where it went: one table, grouped the way you choose.
 * Token counts are engineering detail, so they show only when asked for. */
function UsageBody({ report, agent }: { report: UsageReport; agent: boolean }) {
  const { totals } = report;
  const computers = report.computers?.seconds ? report.computers : undefined;
  const groups: Group[] = [...(agent ? [] : ['agent' as const]), 'model', ...(agent || report.scope === 'mine' ? [] : ['person' as const]), 'day',
    ...(computers && !agent ? ['computers' as const] : [])];
  // With computer time and no model use, the computers are what there is to read.
  const [chosen, setGroup] = useState<Group>(!totals.calls && groups.includes('computers') ? 'computers' : groups[0]);
  const [showTokens, setShowTokens] = useState(false);
  const group = groups.includes(chosen) ? chosen : groups[0];
  if (!totals.calls && !computers) return <p className="ui-help">No model or computer use in the last {report.days} days.</p>;
  const total = report.total_charge_usd ?? totals.charge_usd;
  const rows = { agent: report.by_agent, model: report.by_model, person: report.by_person, day: report.by_day, computers: [] }[group];
  return <>
    {report.scope === 'mine' && <p className="ui-help">The model and computer use made for your requests.</p>}
    <div className="usage-hero" aria-label="Charges"><small>Charges, last {report.days} days</small><strong>{dollars(total)}</strong>
      <span>{[`${tokens(totals.calls)} model ${totals.calls === 1 ? 'request' : 'requests'}`,
        computers && `models ${dollars(totals.charge_usd)}`, computers && `agent computers ${dollars(computers.charge_usd)} for ${workingTime(computers.seconds)}`].filter(Boolean).join(' · ')}</span></div>
    <UsageChart rows={report.by_day} days={report.days} />
    <div className="usage-switch">
      <div className="usage-groups" role="group" aria-label="Group by">{groups.map(id =>
        <button type="button" key={id} aria-pressed={group === id} onClick={() => setGroup(id)}>{groupNames[id][0]}</button>)}</div>
      {group !== 'computers' && <label className="usage-tokens"><input type="checkbox" checked={showTokens} onChange={event => setShowTokens(event.target.checked)} />Show token counts</label>}
    </div>
    {group === 'computers' && computers ? <ComputerTable usage={computers} />
      : <UsageTable caption={groupNames[group][0]} first={groupNames[group][1]} rows={rows} showTokens={showTokens} />}
  </>;
}

function UsageTable({ caption, first, rows, showTokens }: { caption: string; first: string; rows: UsageRow[]; showTokens: boolean }) {
  // Each row's part of what this table lists, so the shares always add up.
  const total = rows.reduce((sum, row) => sum + row.charge_usd, 0);
  return <div className="usage-table-wrap"><table className="usage-table">
    <caption className="visually-hidden">{caption}</caption>
    <thead><tr><th scope="col">{first}</th><th scope="col">Requests</th>{showTokens && <><th scope="col">Tokens read</th><th scope="col">Tokens written</th></>}
      <th scope="col">Charges</th><th scope="col" className="usage-share">Share</th></tr></thead>
    <tbody>{rows.map(row => <tr key={row.key || '-'}>
      <th scope="row">{row.name ?? row.key}</th><td>{tokens(row.calls)}</td>{showTokens && <><td>{tokens(row.input_tokens)}</td><td>{tokens(row.output_tokens)}</td></>}
      <td>{dollars(row.charge_usd)}</td>
      <td className="usage-share"><span className="usage-share-bar" aria-hidden="true"><i style={{ width: share(row.charge_usd, total) }} /></span>{share(row.charge_usd, total)}</td>
    </tr>)}</tbody>
  </table></div>;
}

/** Computer time by agent: how long each agent's computer was awake, and its charge. */
function ComputerTable({ usage }: { usage: ComputerUsage }) {
  return <div className="usage-table-wrap"><table className="usage-table">
    <caption className="visually-hidden">Computers by agent</caption>
    <thead><tr><th scope="col">Agent</th><th scope="col">Working time</th><th scope="col">Charges</th></tr></thead>
    <tbody>{usage.by_agent.map(row => <tr key={row.key}>
      <th scope="row">{row.name ?? row.key}</th><td>{workingTime(row.seconds)}</td><td>{dollars(row.charge_usd)}</td>
    </tr>)}</tbody>
  </table></div>;
}
