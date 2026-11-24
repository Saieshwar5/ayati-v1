import { useCallback } from 'react';
import { useMutation, useResource } from '../../requests';
import { decideProposal, loadImproving, when, type Proposal } from './improving-model';
import { ProposalCard } from './proposal-card';
import { WeekTable } from './week-table';
import './improving.css';

/**
 * Agents › an agent › Improving, for its owners and admins: how its answers went
 * this week against last, skills it proposes (decided here), every three-day
 * review including the ones that changed nothing, the kinds of work it does, and
 * the skills it created with how their work went before and since.
 */
export function ImprovingPanel({ agent, name, changed }: { agent: string; name: string; changed: () => void }) {
  const { data, error, refresh } = useResource(useCallback(() => loadImproving(agent), [agent]));
  const mutation = useMutation();
  if (error && !data) return <p role="alert">{error}</p>;
  if (!data) return <p role="status">Loading how {name} is improving…</p>;
  if (!data.available) return <p className="ui-help">Learning is not available in this workspace yet.</p>;
  const decide = (proposal: Proposal) => (approve: boolean, text?: string) => void mutation.run(async () => {
    const files = text === undefined ? undefined : proposal.files.map(file => file.path === 'SKILL.md' ? { ...file, text } : file);
    await decideProposal(agent, proposal.id, approve, files);
    mutation.setStatus(approve ? `${proposal.name} is now one of ${name}’s skills.` : 'Rejected. The same work won’t be proposed again for 30 days.');
    await refresh();
    changed();
  });
  const open = (data.proposals ?? []).filter(p => p.state === 'proposed');
  const decided = (data.proposals ?? []).filter(p => p.state !== 'proposed');
  return <section className="improving" aria-label={`How ${name} is improving`}>
    {!data.configured?.reviews && <p className="ui-help">{data.configured?.classifier
      ? 'Skill reviews need a learning model on this Cuckoo; answers are still judged.'
      : 'Judging answers and skill reviews need a classifier model on this Cuckoo. Until then only reactions and refused approvals count.'}</p>}
    {data.this_week && data.last_week && <WeekTable now={data.this_week} before={data.last_week} caption={`${name}’s answers this week and last`} />}
    <p className="ui-status" role="status">{mutation.busy ? 'Saving…' : mutation.status}</p>
    {open.length > 0 && <div className="improving-group"><h4>Skills {name} proposes</h4>
      {open.map(p => <ProposalCard key={p.id} agentName={name} proposal={p} busy={mutation.busy} decide={decide(p)} />)}</div>}
    <div className="improving-group"><h4>Reviews every three days</h4>
      {data.reviews?.length ? <ol className="improving-reviews">{data.reviews.map(r => <li key={r.id}><time>{when(r.ran_ms)}</time><span>{r.result}</span></li>)}</ol>
        : <p className="ui-help">{name} reviews its work every three days once it has done at least ten requests since the last review.</p>}</div>
    {(data.skills?.length ?? 0) > 0 && <div className="improving-group"><h4>Skills {name} created</h4>
      <ul className="improving-list">{data.skills!.map(s => <li key={s.id}><span><strong>{s.name}</strong> <span className="improving-made">Created by {name}</span></span>
        <small>{s.job ? `${s.job} · ` : ''}version {s.version} · {when(s.created_ms)}
          {s.before && s.since && s.since.runs > 0 ? ` · corrected ${s.before.corrected} in ${s.before.runs} before, ${s.since.corrected} in ${s.since.runs} since` : ''}</small></li>)}</ul></div>}
    {(data.jobs?.length ?? 0) > 0 && <div className="improving-group"><h4>Kinds of work in the last 30 days</h4>
      <ul className="improving-list">{data.jobs!.map(j => <li key={j.id}><span>{j.name}</span><small>{j.runs} requests · {j.accepted} accepted · {j.corrected} corrected</small></li>)}</ul></div>}
    {decided.length > 0 && <details className="improving-group"><summary>Decided proposals ({decided.length})</summary>
      {decided.map(p => <ProposalCard key={p.id} agentName={name} proposal={p} busy decide={() => {}} />)}</details>}
  </section>;
}
