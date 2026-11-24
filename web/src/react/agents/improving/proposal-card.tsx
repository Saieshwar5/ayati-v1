import { useState } from 'react';
import { Button } from '../../ui';
import type { Proposal } from './improving-model';
import { when } from './improving-model';

const outcomes: Record<string, string> = { accepted: 'accepted', corrected: 'corrected', followed_up: 'followed up', unknown: 'outcome not known' };
const verdicts = { better: 'Better than the real answer', same: 'Same as the real answer', worse: 'Worse than the real answer' };

/** One skill the agent proposes, with why, its evidence, its rehearsal and the
 * draft, which the person can edit before approving. */
export function ProposalCard({ agentName, proposal, busy, decide }: {
  agentName: string; proposal: Proposal; busy: boolean;
  decide: (approve: boolean, text?: string) => void;
}) {
  const main = proposal.files.find(file => file.path === 'SKILL.md');
  const [draft, setDraft] = useState<string>();
  const title = proposal.kind === 'new' ? `New skill: ${proposal.name}` : proposal.kind === 'improve' ? `New version of ${proposal.name}` : `Retire ${proposal.name}`;
  return <article className="improving-proposal" aria-label={title}>
    <header>
      <p className="ui-help">Proposed by {agentName} · {when(proposal.created_ms)}{proposal.job_type ? ` · ${proposal.job_type}` : ''}</p>
      <h4>{title} {proposal.kind !== 'retire' && <span className="improving-made">{proposal.kind === 'new' ? `Created by ${agentName}` : `Improved by ${agentName}`}</span>}</h4>
    </header>
    <p><strong>Why:</strong> {proposal.why}</p>
    {proposal.evidence.length > 0 && <ul className="improving-evidence" aria-label="Requests it came from">
      {proposal.evidence.map((e, i) => <li key={i}>
        <small>{e.conversation ? `In ${e.conversation}` : 'In a conversation you can’t open'} · {when(e.at_ms)} · {outcomes[e.outcome] ?? e.outcome}</small>
        {e.text ? <span>“{e.text}”</span> : <span className="ui-help">Text not shown</span>}
      </li>)}
    </ul>}
    {proposal.rehearsals.length > 0 && <div><h5>Tried on {proposal.rehearsals.length} earlier requests, nothing sent or changed</h5>
      <ul className="improving-rehearsals">{proposal.rehearsals.map((r, i) => <li key={i}><span>{verdicts[r.verdict]}</span>{r.note && <small>{r.note}</small>}</li>)}</ul></div>}
    {main && (draft === undefined
      ? <pre className="improving-draft">{main.text}</pre>
      : <label className="ui-field">SKILL.md<textarea rows={12} value={draft} onChange={event => setDraft(event.target.value)} /></label>)}
    {proposal.state === 'proposed'
      ? <div className="improving-actions">
        <Button primary disabled={busy} onClick={() => decide(true, draft)}>{draft === undefined ? 'Approve' : 'Approve edited skill'}</Button>
        {main && draft === undefined && <Button disabled={busy} onClick={() => setDraft(main.text)}>Edit</Button>}
        <Button disabled={busy} onClick={() => decide(false)}>Reject</Button>
      </div>
      : <p className="ui-help">{proposal.state === 'approved' ? 'Approved' : 'Rejected'}{proposal.decided_by ? ` by ${proposal.decided_by}` : ''}{proposal.decided_ms ? ` · ${when(proposal.decided_ms)}` : ''}</p>}
  </article>;
}
