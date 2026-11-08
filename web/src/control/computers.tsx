import { useCallback, useState } from 'react';
import { useResource } from '../react/requests';
import { control, count, money } from './api';
import { Loading, ReasonDialog, Tag, usePolling } from './shared';

type Route = { provider: string; image: string; usd_per_hour: number };
type Size = { size: string; cpu: number; memory_gb: number; usd_per_hour: number; routes: Route[] };
type ProviderRow = { provider: string; computers: number; awake: number; problems: number; seconds: number; charge: number; cost: number };
type Computer = { workspace: string; workspace_name: string; agent: string; agent_name: string; size: string; provider: string; provider_id: string | null;
  state: 'asleep' | 'awake' | 'deleting'; awake_since: number | null; last_used: number; problem: string | null; in_use: boolean; seconds: number; charge: number; cost: number };
export type OperatorComputers = { configured: boolean; paused: boolean; sizes: Size[] | { error: string } | null; providers: ProviderRow[]; computers: Computer[] };

const load = () => control<OperatorComputers>('/computers');
const hours = (seconds: number) => `${(seconds / 3600).toFixed(1)} h`;
const ago = (unix: number) => unix ? new Date(unix * 1000).toLocaleString() : 'never';

/** Every agent computer (designs/agent-computers 6): providers, sizes and their routes,
 * each computer with its provider and this month's cost, and the switch that pauses
 * them all. Workspaces never see providers or costs. */
export function ComputersScreen() {
  const { data, error, refresh, setData } = useResource(load);
  usePolling(refresh, 15000);
  const [pausing, setPausing] = useState(false);
  const toggle = useCallback(async (reason: string) => {
    if (!data) return;
    await control('/settings', { key: 'computers.paused', value: !data.paused, reason });
    setData({ ...data, paused: !data.paused });
    void refresh().catch(() => {});
  }, [data, refresh, setData]);
  if (!data) return <Loading error={error} label="computers" />;
  const month = data.providers.reduce((sum, p) => ({ charge: sum.charge + p.charge, cost: sum.cost + p.cost, seconds: sum.seconds + p.seconds }), { charge: 0, cost: 0, seconds: 0 });
  const awake = data.providers.reduce((sum, p) => sum + p.awake, 0);
  const problems = data.providers.reduce((sum, p) => sum + p.problems, 0);
  return <div className="cc-stack">
    {!data.configured && <p className="cc-alert" role="note">Computers aren’t set up on this Cuckoo: AYATI_COMPUTER_PROVIDERS_FILE is not set.</p>}
    {data.paused && <p className="cc-alert" role="status">Agents’ computers are paused. None wakes or starts; files are kept.</p>}
    <section className="cc-tiles" aria-label="Computer numbers">
      <div className="cc-card cc-tile"><div className="cc-cap">Awake now</div><div className="cc-tile-value cc-num">{count(awake)}</div>
        <div className="cc-dim cc-small">of {count(data.computers.length)} computers</div></div>
      <div className="cc-card cc-tile"><div className="cc-cap">Working time this month</div><div className="cc-tile-value cc-num">{hours(month.seconds)}</div>
        <div className="cc-dim cc-small">{problems ? `${problems} with a problem` : 'no problems'}</div></div>
      <div className="cc-card cc-tile"><div className="cc-cap">Charged / cost this month</div><div className="cc-tile-value cc-num">{money(month.charge)}</div>
        <div className="cc-dim cc-small">cost {money(month.cost)} · margin {money(month.charge - month.cost)}</div></div>
      <div className="cc-card cc-tile"><div className="cc-cap">Emergency switch</div>
        <button type="button" className={`cc-btn ${data.paused ? 'primary' : 'danger-fill'}`} onClick={() => setPausing(true)}>{data.paused ? 'Resume computers' : 'Pause all computers'}</button></div>
    </section>
    <section className="cc-card" aria-labelledby="cc-computer-providers">
      <div className="cc-card-head"><h2 id="cc-computer-providers">Providers</h2></div>
      <table className="cc-table"><thead><tr><th scope="col">Provider</th><th scope="col">Computers</th><th scope="col">Awake</th><th scope="col">Problems</th><th scope="col">Working time</th><th scope="col">Charged</th><th scope="col">Cost</th></tr></thead>
        <tbody>{data.providers.map(p => <tr key={p.provider}><th scope="row">{p.provider}</th><td>{count(p.computers)}</td><td>{count(p.awake)}</td>
          <td>{p.problems ? <Tag tone="warn">{count(p.problems)}</Tag> : 0}</td><td>{hours(p.seconds)}</td><td>{money(p.charge)}</td><td>{money(p.cost)}</td></tr>)}</tbody></table>
      {!data.providers.length && <p className="cc-dim">No computers yet.</p>}
    </section>
    {Array.isArray(data.sizes) && <section className="cc-card" aria-labelledby="cc-computer-sizes">
      <div className="cc-card-head"><h2 id="cc-computer-sizes">Sizes and routes</h2><span className="cc-dim cc-small">From the providers file; new computers use the first route</span></div>
      <table className="cc-table"><thead><tr><th scope="col">Size</th><th scope="col">CPU · memory</th><th scope="col">Charged / hour</th><th scope="col">Routes (image · cost / hour)</th></tr></thead>
        <tbody>{data.sizes.map(s => <tr key={s.size}><th scope="row">{s.size}</th><td>{s.cpu} · {s.memory_gb} GB</td><td>{money(s.usd_per_hour)}</td>
          <td>{s.routes.map(r => `${r.provider}: ${r.image} · ${money(r.usd_per_hour)}`).join('; ')}</td></tr>)}</tbody></table>
    </section>}
    {data.sizes && !Array.isArray(data.sizes) && <p className="cc-alert" role="alert">The providers file can’t be read: {data.sizes.error}</p>}
    <section className="cc-card" aria-labelledby="cc-computer-list">
      <div className="cc-card-head"><h2 id="cc-computer-list">Computers</h2><span className="cc-dim cc-small">Awake and troubled first</span></div>
      <table className="cc-table"><thead><tr><th scope="col">Agent</th><th scope="col">Workspace</th><th scope="col">State</th><th scope="col">Size · provider</th><th scope="col">Last used</th><th scope="col">This month</th></tr></thead>
        <tbody>{data.computers.map(c => <tr key={`${c.workspace}/${c.agent}`}>
          <th scope="row">{c.agent_name}</th><td>{c.workspace_name}</td>
          <td>{c.state === 'awake' ? <Tag tone="ok">{c.in_use ? 'Working' : 'Awake'}</Tag> : c.state === 'deleting' ? <Tag tone="warn">Being reset</Tag> : <Tag tone="off">Asleep</Tag>}
            {c.problem && <div className="cc-dim cc-small" title={c.problem}>Problem: {c.problem.slice(0, 80)}</div>}</td>
          <td>{c.size} · {c.provider}{c.provider_id && <div className="cc-dim cc-small">{c.provider_id}</div>}</td>
          <td>{ago(c.last_used)}</td><td>{hours(c.seconds)} · {money(c.charge)} <span className="cc-dim">(cost {money(c.cost)})</span></td>
        </tr>)}</tbody></table>
      {!data.computers.length && <p className="cc-dim">No agent has a computer yet.</p>}
    </section>
    {pausing && <ReasonDialog title={data.paused ? 'Resume agents’ computers?' : 'Pause every agent’s computer?'} danger={!data.paused}
      confirm={data.paused ? 'Resume computers' : 'Pause all computers'} cli={`ayati admin setting set computers.paused ${!data.paused} --reason "…"`}
      onConfirm={toggle} onClose={() => setPausing(false)}>
      <p>{data.paused ? 'Agents can wake and start their computers again.' : 'No computer wakes or starts, and awake ones go to sleep within a minute. Agents are told computers are paused. Files are kept.'}</p>
    </ReasonDialog>}
  </div>;
}
