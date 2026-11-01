import { useCallback, useState } from 'react';
import { Button } from '../ui';
import { useMutation } from '../requests';
import { identityRequest, workspaceId } from '../../workspace-context';
import { dayLabel, type Scope } from './model';
import { useLive } from './use-live';

// Activity → Review (people-and-agents-first, Phase 5): "Can I trust agents with
// more?" — measures, rules from repeated decisions, goals and weekly reviews, for
// the people who can fix the agents.
type Measure = { id: string; name: string; tasks: number; done: number; failed: number; open: number; due_done: number; on_time: number;
  needed_person: number; person_minutes: number; requests?: number; model_runs?: number };
type Suggestion = { agent: string; agent_name: string; connection: string; operation: string; title: string; approved: number };
type Goal = { id: string; agent: string; agent_name: string; name: string; target: number; unit: string; period: 'week' | 'month'; current: number | null; risk: string | null; updated_ms: number | null };
type Review = { id: string; agent: string; agent_name: string; done: string; went_wrong: string; learned: string; suggestion: string | null; at: number };
type Weekly = { agent: string; enabled: boolean; next_ms: number | null };
export type ReviewPage = { available: boolean; days?: number; agents?: Measure[]; people?: Measure[]; suggestions?: Suggestion[]; goals?: Goal[]; reviews?: Review[]; weekly?: Weekly[] };

const root = () => `/workspaces/${workspaceId}/activity`;
const load = (scope: Scope) => identityRequest<ReviewPage>(`${root()}/review?days=30&scope=${scope}`);

export function ReviewView({ scope }: { scope: Scope }) {
  const { data, error, refresh } = useLive(useCallback(() => load(scope), [scope]));
  const m = useMutation();
  const act = (path: string, body: unknown, done: string) => void m.run(async () => { await identityRequest(`${root()}${path}`, body); await refresh(); m.setStatus(done); });
  if (error) return <p role="alert">{error}</p>;
  if (!data) return <p role="status">Loading the review…</p>;
  if (!data.available) return <p className="activity-empty">Reviews need the latest workspace database.</p>;
  const agents = data.agents ?? [];
  const weekly = (agent: string) => data.weekly?.find(w => w.agent === agent);
  return <section className="activity-review" aria-label="Review">
    <p className="ui-status" role="status">{m.status}</p>
    {Boolean(data.suggestions?.length) && <section aria-label="Suggested rules"><h2 className="activity-group">Let agents do more</h2>
      <ul className="activity-list">{data.suggestions!.map(s => <li key={`${s.agent}.${s.connection}.${s.operation}`}>
        <span className="activity-main"><strong>{s.agent_name} asked {s.approved} times to {s.title.toLowerCase()}, and each was approved.</strong>
          <small>Let {s.agent_name} do it without asking? You can change it back on its Tools tab. It can never go beyond your admin's rule.</small></span>
        <Button primary disabled={m.busy} onClick={() => act('/rule-suggestions', { agent: s.agent, connection: s.connection, operation: s.operation, accept: true }, `${s.agent_name} no longer asks for this.`)}>Let it</Button>
        <Button disabled={m.busy} onClick={() => act('/rule-suggestions', { agent: s.agent, connection: s.connection, operation: s.operation, accept: false }, 'It keeps asking.')}>Keep asking</Button>
      </li>)}</ul></section>}
    <section aria-label="How agents are doing"><h2 className="activity-group">Last 30 days</h2>
      {agents.length ? <div className="activity-table-wrap"><table className="activity-table"><thead><tr><th>Agent</th><th>Tasks done</th><th>Failed</th><th>On time</th>
        <th>Needed a person</th><th>Person-minutes</th><th>Requests</th><th>Model runs</th><th>Weekly review</th></tr></thead>
        <tbody>{agents.map(a => <tr key={a.id}><th scope="row">{a.name}</th><td>{a.done} of {a.tasks}</td><td>{a.failed}</td>
          <td>{a.due_done ? `${a.on_time} of ${a.due_done}` : '—'}</td><td>{a.needed_person}</td><td>{a.person_minutes}</td><td>{a.requests ?? 0}</td><td>{a.model_runs ?? 0}</td>
          <td><Button disabled={m.busy} aria-label={`Weekly review for ${a.name}`} onClick={() => act('/weekly-review', { agent: a.id, enabled: !weekly(a.id)?.enabled },
            weekly(a.id)?.enabled ? `Weekly review for ${a.name} is off.` : `${a.name} will write a review every week.`)}>{weekly(a.id)?.enabled ? 'On · turn off' : 'Turn on'}</Button></td></tr>)}</tbody></table></div>
        : <p className="activity-empty">No agents you manage yet.</p>}
      <small className="ui-help">Person-minutes: time from reading a question or approval to answering it, at most ten minutes each. Model runs are counts; cost in money comes later.</small>
    </section>
    {Boolean(data.people?.length) && <section aria-label="By person"><h2 className="activity-group">By person</h2>
      <ul className="activity-list">{data.people!.map(p => <li key={p.id}><span className="activity-main"><strong>{p.name}</strong>
        <small>{p.tasks} tasks · {p.done} done · {p.needed_person} needed them · {p.person_minutes} minutes</small></span></li>)}</ul></section>}
    <Goals goals={data.goals ?? []} agents={agents} busy={m.busy} act={act} />
    <section aria-label="Weekly reviews"><h2 className="activity-group">Weekly reviews</h2>
      {data.reviews?.length ? data.reviews.map(r => <article key={r.id} className="activity-review-card"><h3>{r.agent_name} · {dayLabel(r.at)}</h3>
        <p><strong>Done:</strong> {r.done}</p><p><strong>Went wrong:</strong> {r.went_wrong}</p><p><strong>Learned:</strong> {r.learned}</p>
        {r.suggestion && <p className="activity-fix"><strong>Suggested change:</strong> {r.suggestion} <a href={`?view=agents&selected=${encodeURIComponent(r.agent)}`}>Set up {r.agent_name} →</a></p>}
      </article>) : <p className="activity-empty">Turn on a weekly review above; each agent then writes one for its managers.</p>}
    </section>
  </section>;
}

function Goals({ goals, agents, busy, act }: { goals: Goal[]; agents: Measure[]; busy: boolean; act: (path: string, body: unknown, done: string) => void }) {
  const [form, setForm] = useState({ agent: '', name: '', target: '', unit: '', period: 'week' });
  return <section aria-label="Goals"><h2 className="activity-group">Goals</h2>
    <ul className="activity-list">{goals.map(g => {
      const share = g.current === null ? 0 : Math.min(100, Math.round(g.current / g.target * 100));
      return <li key={g.id} className="activity-goal"><span className="activity-main"><strong>{g.agent_name}: {g.name}</strong>
        <small>{g.current ?? 0} of {g.target} {g.unit} this {g.period}{g.risk ? ` · at risk: ${g.risk}` : ''}</small>
        <span className={`activity-goal-bar${g.risk ? ' problem' : ''}`} role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={share}><span style={{ width: `${share}%` }} /></span></span>
        <Button disabled={busy} onClick={() => act(`/goals/${encodeURIComponent(g.id)}/archive`, {}, 'Goal removed.')}>Remove</Button></li>;
    })}</ul>
    {agents.length > 0 && <form className="activity-goal-form" onSubmit={event => { event.preventDefault();
      act('/goals', { agent: form.agent || agents[0].id, name: form.name, target: Number(form.target), unit: form.unit, period: form.period }, 'Goal saved. The agent reports progress as it works.');
      setForm(f => ({ ...f, name: '', target: '' })); }}>
      <select aria-label="Agent" value={form.agent || agents[0].id} onChange={e => setForm({ ...form, agent: e.target.value })}>{agents.map(a => <option key={a.id} value={a.id}>{a.name}</option>)}</select>
      <input aria-label="Goal" placeholder="Quotes sent" maxLength={80} required value={form.name} onChange={e => setForm({ ...form, name: e.target.value })} />
      <input aria-label="Target" type="number" min="1" step="any" placeholder="50" required value={form.target} onChange={e => setForm({ ...form, target: e.target.value })} />
      <input aria-label="Unit" placeholder="quotes" maxLength={16} value={form.unit} onChange={e => setForm({ ...form, unit: e.target.value })} />
      <select aria-label="Period" value={form.period} onChange={e => setForm({ ...form, period: e.target.value })}><option value="week">per week</option><option value="month">per month</option></select>
      <Button type="submit" disabled={busy}>Add goal</Button>
    </form>}
  </section>;
}
