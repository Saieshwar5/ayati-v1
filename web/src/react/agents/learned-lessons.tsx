import { useCallback, useState } from 'react';
import { identityRequest } from '../../workspace-context';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { base, type Agent } from './model';

type Evidence = { kind: 'answer' | 'reaction' | 'rejected_approval' | 'owner_request'; excerpt?: string; conversation?: string };
export type Lesson = {
  id: string; title: string; content: string; why: string;
  kind: 'new' | 'update' | 'conflict'; state: 'proposed' | 'approved' | 'rejected'; created_ms: number;
  evidence: Evidence[]; conversation: { id: string; name: string } | null;
  replaces: { id: string; title: string; content: string } | null;
};
type Learning = {
  available: boolean; configured?: boolean; enabled?: boolean; revision?: number;
  last_run_ms?: number | null; last_result?: string | null; proposals: Lesson[];
};
type Draft = { id: string; title: string; content: string };

const evidenceLabels: Record<Evidence['kind'], string> = {
  answer: 'A person answered its reply',
  reaction: 'A person reacted to its reply',
  rejected_approval: 'A person refused to approve',
  owner_request: 'An owner or admin asked for it',
};

/** Knowledge → Lessons from its work (schema 121): once a day Cuckoo proposes
 * lessons from the agent's work in shared conversations. Nothing reaches the
 * agent until a person who maintains it approves, as proposed or edited. */
export function LearnedLessons({ agent, saved }: { agent: Agent; saved: () => Promise<void> }) {
  const path = `${base}/${agent.id}/learning`;
  const load = useCallback(() => identityRequest<Learning>(path), [path]);
  const learning = useResource(load);
  const mutation = useMutation();
  const [draft, setDraft] = useState<Draft>();
  const [everywhere, setEverywhere] = useState<Record<string, boolean>>({});
  const data = learning.data;
  const open = data?.proposals?.filter(lesson => lesson.state === 'proposed') ?? [];
  const decided = data?.proposals?.filter(lesson => lesson.state !== 'proposed').slice(0, 5) ?? [];

  const decide = (lesson: Lesson, approve: boolean, edited?: Draft) => void mutation.run(async () => {
    await identityRequest(`${path}/${lesson.id}`, {
      approve, everywhere: Boolean(everywhere[lesson.id]),
      ...(edited ? { title: edited.title, content: edited.content } : {}),
    });
    setDraft(undefined);
    await learning.refresh().catch(() => {});
    if (approve) await saved();
    mutation.setStatus(approve ? `“${edited?.title ?? lesson.title}” is now part of ${agent.name}’s knowledge.` : `“${lesson.title}” was rejected.`);
  });
  const switchLearning = (enabled: boolean) => void mutation.run(async () => {
    learning.setData(await identityRequest<Learning>(path, { enabled, revision: data?.revision ?? 0 }));
    mutation.setStatus(enabled ? 'Cuckoo will suggest lessons from this agent’s work.' : 'No new lessons will be suggested.');
  });

  return <section className="agent-lessons" aria-labelledby={`lessons-${agent.id}`}>
    <h3 id={`lessons-${agent.id}`}>Lessons from its work</h3>
    {learning.error && <p role="alert">{learning.error}</p>}
    {!data && !learning.error && <p role="status">Loading lessons…</p>}
    {data && !data.available && <p className="ui-help">Run workspace-db migrate to use lessons.</p>}
    {data?.available && <>
      {!data.configured && <p className="ui-help" role="note">Learning isn’t set up on this Cuckoo yet, so no lessons are suggested.</p>}
      <div className="agent-lesson-switch">
        <span><strong>Suggest lessons</strong></span>
        <label className="ui-switch"><input type="checkbox" role="switch" checked={Boolean(data.enabled)} disabled={mutation.busy}
          onChange={event => switchLearning(event.target.checked)} /><span>Suggest lessons</span></label>
      </div>
      {data.last_run_ms && <p className="ui-help">Last checked {new Date(data.last_run_ms).toLocaleString()}: {data.last_result}</p>}
      {!open.length && <p className="ui-help">No lessons waiting.</p>}
      {open.map(lesson => <article key={lesson.id} aria-label={lesson.title}>
        {draft?.id === lesson.id ? <form className="ui-form" aria-label={`Edit ${lesson.title}`}
          onSubmit={event => { event.preventDefault(); decide(lesson, true, draft); }}>
          <label className="ui-field">Title<input required maxLength={160} value={draft.title} disabled={mutation.busy}
            onChange={event => setDraft({ ...draft, title: event.target.value })} /></label>
          <label className="ui-field">Lesson<textarea required maxLength={2000} rows={3} value={draft.content} disabled={mutation.busy}
            onChange={event => setDraft({ ...draft, content: event.target.value })} /></label>
          <div className="agent-knowledge-actions">
            <Button primary type="submit" disabled={mutation.busy || !draft.title.trim() || !draft.content.trim()}>Approve edited lesson</Button>
            <Button disabled={mutation.busy} onClick={() => setDraft(undefined)}>Cancel</Button>
          </div>
        </form> : <>
          <strong>{lesson.title}</strong>
          <small>{lesson.kind === 'new' ? 'New lesson'
            : lesson.kind === 'update' ? `Updates “${lesson.replaces?.title}”` : `Disagrees with “${lesson.replaces?.title}”`}</small>
          <p>{lesson.content}</p>
          {lesson.replaces && <p className="ui-help">Now: {lesson.replaces.content}</p>}
          {lesson.why && <p className="ui-help">Why: {lesson.why}</p>}
          <ul className="agent-lesson-evidence" aria-label="Evidence">
            {lesson.evidence.map((item, index) => <li key={index}>{evidenceLabels[item.kind]}
              {item.excerpt ? <> in {item.conversation}: “{item.excerpt}”</> : ' in a conversation you can’t open'}</li>)}
          </ul>
          {lesson.kind === 'new' && <fieldset className="agent-lesson-audience"><legend>Use it</legend>
            {lesson.conversation && <label className="ui-check"><input type="radio" name={`audience-${lesson.id}`} checked={!everywhere[lesson.id]}
              onChange={() => setEverywhere({ ...everywhere, [lesson.id]: false })} />Only in {lesson.conversation.name}</label>}
            <label className="ui-check"><input type="radio" name={`audience-${lesson.id}`} checked={!lesson.conversation || Boolean(everywhere[lesson.id])}
              disabled={!lesson.conversation} onChange={() => setEverywhere({ ...everywhere, [lesson.id]: true })} />
              Every conversation with {agent.name}</label>
          </fieldset>}
          <div className="agent-knowledge-actions">
            <Button primary disabled={mutation.busy} onClick={() => decide(lesson, true)}>Approve</Button>
            <Button disabled={mutation.busy} onClick={() => setDraft({ id: lesson.id, title: lesson.title, content: lesson.content })}>Edit</Button>
            <Button disabled={mutation.busy} onClick={() => decide(lesson, false)}>Reject</Button>
          </div>
        </>}
      </article>)}
      {decided.length > 0 && <details><summary>Recently decided</summary>
        <ul>{decided.map(lesson => <li key={lesson.id}>{lesson.title} · {lesson.state === 'approved' ? 'Approved' : 'Rejected'}</li>)}</ul>
      </details>}
      <p className="ui-status" role="status">{mutation.busy ? 'Saving…' : mutation.status}</p>
    </>}
  </section>;
}
