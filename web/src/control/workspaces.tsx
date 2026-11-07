import { useCallback, useEffect, useState } from 'react';
import { useResource } from '../react/requests';
import { ago, control, count, money, when, type WorkspaceDetail, type WorkspaceRow, type WorkspaceStatus } from './api';
import type { Go } from './app';
import { Loading, ReasonDialog, Sparkline, Tag } from './shared';

const TONE: Record<WorkspaceStatus, 'ok' | 'warn' | 'bad' | 'off' | 'new'> = { active: 'ok', paused: 'warn', closing: 'off', suspended: 'bad', closed: 'off' };
type Filter = 'all' | 'near-cap' | 'errors' | 'suspended';

export function WorkspacesScreen({ go, initialQuery, selected }: { go: Go; initialQuery: string; selected: string | null }) {
  const [query, setQuery] = useState(initialQuery);
  const [search, setSearch] = useState(initialQuery);
  const [filter, setFilter] = useState<Filter>('all');
  // Wait for a pause in typing before asking the server.
  useEffect(() => { const timer = setTimeout(() => setSearch(query), 300); return () => clearTimeout(timer); }, [query]);
  const load = useCallback(() => control<{ workspaces: WorkspaceRow[]; total: number }>(`/workspaces?q=${encodeURIComponent(search)}`), [search]);
  const { data, error, refresh } = useResource(load);
  const nearCap = (w: WorkspaceRow) => w.budget !== null && w.budget > 0 && w.charge / w.budget >= 0.8;
  const rows = (data?.workspaces ?? []).filter(w => filter === 'all' || (filter === 'near-cap' && nearCap(w))
    || (filter === 'errors' && w.error_rate_7d >= 2) || (filter === 'suspended' && w.status === 'suspended'));
  const counts = {
    'near-cap': data?.workspaces.filter(nearCap).length ?? 0,
    errors: data?.workspaces.filter(w => w.error_rate_7d >= 2).length ?? 0,
    suspended: data?.workspaces.filter(w => w.status === 'suspended').length ?? 0,
  };
  const current = selected ?? rows[0]?.id ?? null;
  return <div className="cc-stack">
    <div className="cc-toolbar">
      <label className="cc-visually-hidden" htmlFor="cc-ws-search">Find workspaces</label>
      <input id="cc-ws-search" className="cc-input" type="search" placeholder="Name, owner email or id" value={query} onChange={event => setQuery(event.target.value)} />
      {(['all', 'near-cap', 'errors', 'suspended'] as Filter[]).map(f => <button key={f} type="button" className="cc-chip" aria-pressed={filter === f} onClick={() => setFilter(f)}>
        {f === 'all' ? 'All' : f === 'near-cap' ? 'Near budget' : f === 'errors' ? 'Errors ≥ 2%' : 'Suspended'} <span className="cc-dim cc-num">{f === 'all' ? data?.total ?? '' : counts[f]}</span>
      </button>)}
    </div>
    <div className="cc-row cc-top-align">
      <section className="cc-card cc-grow-3 cc-scroll" aria-label="Workspaces">
        {!data ? <Loading error={error} label="workspaces" /> : <table className="cc-table cc-select">
          <thead><tr><th scope="col">Workspace</th><th scope="col">People</th><th scope="col">Agents</th><th scope="col">Calls · 7 days</th><th scope="col">Charged this month</th><th scope="col">Errors</th><th scope="col">Status</th></tr></thead>
          <tbody>{rows.map(w => <tr key={w.id} aria-selected={w.id === current}>
            <td><button type="button" className="cc-row-button" onClick={() => go('workspaces', { id: w.id, ...(search ? { q: search } : {}) })}>
              <strong>{w.name}</strong><span className="cc-mono cc-dim">{w.owner ?? 'no admin'} · {w.id.slice(0, 8)}</span></button></td>
            <td className="cc-num">{w.people}</td><td className="cc-num">{w.agents}</td>
            <td><Sparkline values={w.calls_7d} /></td>
            <td><BudgetMeter charge={w.charge} budget={w.budget} /></td>
            <td className={`cc-num${w.error_rate_7d >= 2 ? ' bad' : ''}`}>{w.error_rate_7d.toFixed(1)}%</td>
            <td><Tag tone={TONE[w.status]}>{w.status.toUpperCase()}</Tag></td>
          </tr>)}</tbody>
        </table>}
        {data && !rows.length && <p className="cc-empty">No workspaces match.</p>}
      </section>
      {current && <WorkspacePanel key={current} id={current} onChanged={() => void refresh().catch(() => {})} />}
    </div>
  </div>;
}

/** Charged this month against the workspace's own model budget (set by its admins). */
export function BudgetMeter({ charge, budget }: { charge: number; budget: number | null }) {
  const pct = budget ? Math.min(100, 100 * charge / budget) : 0;
  return <div className="cc-meter"><span className="cc-num">{money(charge)} <span className="cc-dim">/ {budget === null ? 'no budget' : money(budget, 0)}</span></span>
    {budget !== null && <span className="cc-meter-track" role="img" aria-label={`${pct.toFixed(0)}% of the budget`}><span className={pct >= 80 ? 'warn' : ''} style={{ width: `${pct}%` }} /></span>}</div>;
}

type Action = 'suspend' | 'resume';

function WorkspacePanel({ id, onChanged }: { id: string; onChanged: () => void }) {
  const load = useCallback(() => control<WorkspaceDetail>(`/workspaces/${encodeURIComponent(id)}`), [id]);
  const { data, error, setData } = useResource(load);
  const [action, setAction] = useState<Action>();
  if (!data) return <aside className="cc-card cc-grow-1 cc-pad"><Loading error={error} label="the workspace" /></aside>;
  const save = async (path: string, body: object) => {
    setData(await control<WorkspaceDetail>(`/workspaces/${encodeURIComponent(id)}/${path}`, body));
    onChanged();
  };
  return <aside className="cc-card cc-grow-1" aria-labelledby="cc-ws-title">
    <div className="cc-pad cc-divide">
      <div className="cc-between"><h2 id="cc-ws-title">{data.name}</h2><Tag tone={TONE[data.status]}>{data.status.toUpperCase()}</Tag></div>
      <div className="cc-mono cc-dim">{data.id}</div>
    </div>
    <dl className="cc-kv cc-pad cc-divide">
      <dt>Created</dt><dd>{when(data.created_ms)}</dd>
      <dt>Owner</dt><dd>{data.owner ?? '—'}</dd>
      <dt>People</dt><dd className="cc-num">{data.people} · {data.admins} admins</dd>
      <dt>Agents</dt><dd className="cc-num">{data.agents} · {data.running} running</dd>
      <dt>Models used</dt><dd className="cc-mono">{data.models.join(', ') || '—'}</dd>
      <dt>Connected apps</dt><dd className="cc-num">{data.connections}</dd>
      <dt>Library</dt><dd className="cc-num">{count(data.library_items)} items · {(data.library_bytes / 1e6).toFixed(1)} MB</dd>
      <dt>This month</dt><dd><BudgetMeter charge={data.charge} budget={data.budget} /><span className="cc-dim cc-small">Provider cost {money(data.cost)} · budget set by its admins</span></dd>
      <dt>Last model call</dt><dd>{ago(data.last_active_ms)}</dd>
      <dt>Problem reports</dt><dd className="cc-num">{data.problem_reports_30d} in 30 days</dd>
      <dt>Signed up with</dt><dd>{data.code ?? 'no invite code'}</dd>
      {data.suspended_note && <><dt>Suspended</dt><dd className="bad">{data.suspended_note} · {when(data.suspended_ms)}</dd></>}
    </dl>
    <div className="cc-pad cc-stack-tight">
      <div className="cc-cap">Actions</div>
      {data.status === 'suspended'
        ? <button type="button" className="cc-btn" onClick={() => setAction('resume')}>Lift the suspension</button>
        : <button type="button" className="cc-btn danger" onClick={() => setAction('suspend')}>Suspend workspace…</button>}
      <div className="cc-cli"><span aria-hidden="true">$</span> ayati admin workspace show {data.id}</div>
      <p className="cc-dim cc-small">You see counts, names and spend here. Messages and Library files stay private to the workspace.</p>
    </div>
    {data.log.length > 0 && <div className="cc-pad cc-divide-top"><div className="cc-cap">Operator changes</div>
      <ul className="cc-history">{data.log.map(entry => <li key={`${entry.at_ms}-${entry.summary}`}><span className="cc-dim cc-num">{when(entry.at_ms)}</span> {entry.by} {entry.summary}<span className="cc-dim"> — “{entry.reason}”</span></li>)}</ul></div>}
    {action === 'suspend' && <ReasonDialog title={`Suspend ${data.name}?`} confirm="Suspend workspace" danger
      cli={`ayati admin workspace suspend ${data.id} --reason "…"`} onClose={() => setAction(undefined)} onConfirm={reason => save('suspend', { reason })}>
      <ul className="cc-impact">
        <li>None of its {data.people} members can open it.</li>
        <li>{data.running ? `${data.running} running request(s) stop now; ` : ''}agents take no new work.</li>
        <li>Nothing is deleted. Lifting the suspension lets everyone back in.</li>
      </ul>
    </ReasonDialog>}
    {action === 'resume' && <ReasonDialog title={`Lift the suspension of ${data.name}?`} confirm="Lift suspension"
      cli={`ayati admin workspace resume ${data.id} --reason "…"`} onClose={() => setAction(undefined)} onConfirm={reason => save('resume', { reason })}>
      <p className="cc-dim">Members can open it again. Agents resume unless an admin paused them before.</p>
    </ReasonDialog>}
  </aside>;
}
