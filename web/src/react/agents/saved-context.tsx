import { useState } from 'react';
import { Button } from '../ui';
import { useSavedContext } from './use-saved-context';
import type { ContextPreview } from './use-context-preview';

type Entry = { text: string; sources: string[] };
export type WorkingContext = { revision: number; available: boolean; brief?: {
  objective: Entry; decisions: Entry[]; questions: Entry[]; corrections: Entry[];
} };
export type ContextAttempt = {
  task: string; claim: string; created_at: number; available: boolean; model?: string; source_count?: number;
  evidence?: { working?: WorkingContext; profile_revision?: number; conversation: { request_message: string; messages: ContextPreview['messages'] }; notes: ContextPreview['notes'] };
  reads?: { sequence: number; tool: string; result: { messages?: { id: string; excerpt: string }[];
    message?: { id: string; content: string } }; at: number }[];
};
function SummaryEntry({ entry }: { entry: Entry }) {
  return <li><p>{entry.text}</p><small>Evidence: {entry.sources.join(', ')}</small></li>;
}
export function WorkingSummary({ working, label = 'Working summary' }: { working?: WorkingContext; label?: string }) {
  if (!working?.revision) return <p className="ui-help">No working summary yet. The agent can use recent messages and retrieve older eligible history.</p>;
  return <section aria-label="Working summary"><h3>{label} · Revision {working.revision}</h3>
    {!working.available || !working.brief ? <p>Outdated or unavailable. New work uses fresh evidence.</p> : <>
      <p className="ui-help">Agent interpretation for your work with this agent in this conversation. Original evidence and controller action records take precedence.</p>
      <h4>Objective</h4><ul><SummaryEntry entry={working.brief.objective} /></ul>
      {([['Reported decisions', working.brief.decisions], ['Open questions', working.brief.questions],
        ['Corrections and conflicts', working.brief.corrections]] as const).map(([label, entries]) =>
        <div key={label}><h4>{label}</h4>{entries.length ? <ul>{entries.map((entry, i) => <SummaryEntry key={i} entry={entry} />)}</ul> : <p className="ui-help">None recorded.</p>}</div>)}
    </>}
  </section>;
}
function SavedAttempt({ attempt, agent, chat }: { attempt: ContextAttempt; agent: string; chat: string }) {
  const [open, setOpen] = useState(false);
  const { data, error } = useSavedContext(agent, chat, attempt, open);
  const date = new Date(attempt.created_at * 1000);
  return <article><strong><time dateTime={date.toISOString()}>{date.toLocaleString()}</time></strong>
    <small>Task {attempt.task} · {attempt.available ? 'Available' : 'Outdated or unavailable'}</small>
    {attempt.available ? <>
      <Button aria-expanded={open} onClick={() => setOpen(!open)}>{open ? 'Hide saved evidence' : 'View saved evidence'}</Button>
      {open && !data && !error && <p role="status">Loading saved evidence…</p>}
      {open && error && <p role="alert">{error}</p>}
      {open && data && !data.available && <p>Outdated or unavailable. Close and refresh the context view.</p>}
      {open && data?.available && <div>
        <p className="ui-help">Input recorded for this attempt, before inference. Later history reads are recorded below. This does not prove the model followed or used every source.</p>
        <small>Model: {data.model} · Other protected task sources: {data.source_count ?? 0}</small>
        <p className="ui-help">Agent profile revision: {data.evidence?.profile_revision ?? 'Not recorded'}</p>
        <WorkingSummary working={data.evidence?.working} label="Summary included in this attempt" />
        <h4>Selected messages</h4>
        {data.evidence?.conversation?.messages?.map(message => <article key={message.id}>
          <strong>{message.author.display_name || message.author.kind}</strong><small>Message {message.id}</small><p>{message.content}</p>
        </article>)}
        <h4>Approved note versions</h4>
        {data.evidence?.notes.map(note => <article key={note.id}><strong>{note.title}</strong><small>Revision {note.revision} · {note.id}</small><p>{note.content}</p></article>)}
        <h4>Later history reads</h4>
        {!data.reads?.length && <p className="ui-help">No recorded history reads.</p>}
        {data.reads?.map(read => <article key={read.sequence}><strong>{read.tool}</strong>
          {read.result.messages?.map(message => <p key={message.id}>{message.excerpt}<small>Message {message.id}</small></p>)}
          {typeof read.result.message === 'object' && <p>{read.result.message.content}<small>Message {read.result.message.id}</small></p>}
        </article>)}
      </div>}
    </> : <p className="ui-help">Saved text is hidden because a source or access rule changed. Restoring permission does not revive this snapshot.</p>}
  </article>;
}
export function SavedContext({ attempts, agent, chat }: { attempts?: ContextAttempt[]; agent: string; chat: string }) {
  return <section aria-label="Saved task context"><h3>Saved task context</h3>
    <p className="ui-help">The latest 20 attempts for your requests in this conversation. Tasks before context recording was introduced have no saved snapshot.</p>
    {!attempts?.length && <p>No saved task context.</p>}
    {attempts?.map(attempt => <SavedAttempt key={`${attempt.task}:${attempt.claim}:${attempt.available}`} attempt={attempt} agent={agent} chat={chat} />)}
  </section>;
}
