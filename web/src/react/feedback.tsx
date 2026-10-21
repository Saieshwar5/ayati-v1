import { useCallback, useRef, useState } from 'react';
import { chatRequest, type Conversation } from './chat/model';
import { currentWorkspace, workspaceUrl } from '../workspace-context';
import { Button, Dialog } from './ui';
import { useMutation, useResource } from './requests';
type Finding = { id: string; chat: string; title: string; expected: string; actual: string; category: string; severity: string; status: string; evidence: string; revision: number };
export function FeedbackPage() {
  const load = useCallback(async () => {
    const [issues, chats] = await Promise.all([chatRequest<{ feedback: Finding[] }>('/feedback'), chatRequest<{ chats: Conversation[] }>('/chats')]);
    return { ...issues, ...chats };
  }, []);
  const resource = useResource(load);
  const [create, setCreate] = useState(false);
  const [filter, setFilter] = useState('all');
  const mutation = useMutation();
  const manager = ['Owner', 'Admin'].includes(currentWorkspace()?.role ?? '');
  return <main className="company-page"><header><h1>Feedback</h1><Button primary onClick={() => setCreate(true)}>Report an issue</Button></header>
    <p>Reports are visible only to members of the linked conversation. The latest 200 accessible reports are shown.</p>
    <label>Status <select value={filter} onChange={e => setFilter(e.target.value)}>{['all', 'new', 'investigating', 'fixed', 'verified', 'deferred'].map(s => <option key={s}>{s}</option>)}</select></label>
    {(resource.error || mutation.status) && <p role="alert">{resource.error || mutation.status}</p>}
    {!resource.data && !resource.error && <p role="status">Loading feedback…</p>}
    <div className="company-rows">{!resource.error && resource.data?.feedback.filter(f => filter === 'all' || f.status === filter).map(f => <article key={f.id}>
      <header><h2>{f.title}</h2><span>{f.severity} · {f.category}</span></header>
      <p><strong>Expected:</strong> {f.expected}</p><p><strong>Observed:</strong> {f.actual}</p><pre>{f.evidence}</pre>
      <a href={workspaceUrl(`/?view=chat&conversation=${f.chat}`)}>Open conversation</a>
      {manager ? <label>Status <select aria-label={`Status for ${f.title}`} disabled={mutation.busy} value={f.status} onChange={e => { const status = e.target.value; void mutation.run(async () => { await chatRequest(`/feedback/${f.id}`, { status, revision: f.revision }); await resource.refresh(); }); }}>{['new', 'investigating', 'fixed', 'verified', 'deferred'].map(s => <option key={s}>{s}</option>)}</select></label> : <p>{f.status}</p>}
    </article>)}</div>
    {resource.data?.feedback.length === 0 && <p>No issues reported yet.</p>}
    {create && <FeedbackForm chats={resource.data?.chats ?? []} close={() => setCreate(false)} saved={async () => { await resource.refresh(); setCreate(false); }} />}
  </main>;
}
function FeedbackForm({ chats, close, saved }: { chats: Conversation[]; close: () => void; saved: () => Promise<void> }) {
  const [draft, setDraft] = useState({ chat: chats[0]?.id ?? '', title: '', expected: '', actual: '', category: 'bug', severity: 'medium', evidence: '' });
  const mutation = useMutation();
  const attempt = useRef<{ payload: string; id: string }>(undefined);
  return <Dialog title="Report an issue" close={close} busy={mutation.busy} status={mutation.status}>
    <form className="company-form" onSubmit={e => { e.preventDefault(); void mutation.run(async () => {
      const payload = JSON.stringify(draft); if (attempt.current?.payload !== payload) attempt.current = { payload, id: crypto.randomUUID() };
      await chatRequest('/feedback', { ...draft, id: attempt.current.id }); await saved();
    }); }}>
      <label>Conversation<select required value={draft.chat} onChange={e => setDraft({ ...draft, chat: e.target.value })}>{chats.map(c => <option key={c.id} value={c.id}>{c.name}</option>)}</select></label>
      {(['title', 'expected', 'actual', 'evidence'] as const).map(field => <label key={field}>{({ title: 'Issue', expected: 'Expected result', actual: 'Actual result', evidence: 'Evidence or reproduction steps' })[field]}<textarea required={field !== 'evidence'} maxLength={field === 'title' ? 160 : 4000} value={draft[field]} onChange={e => setDraft({ ...draft, [field]: e.target.value })} /></label>)}
      <label>Category<select value={draft.category} onChange={e => setDraft({ ...draft, category: e.target.value })}>{['bug', 'performance', 'missing-feature', 'usability', 'reliability', 'architecture'].map(v => <option key={v}>{v}</option>)}</select></label>
      <label>Impact<select value={draft.severity} onChange={e => setDraft({ ...draft, severity: e.target.value })}>{['low', 'medium', 'high', 'critical'].map(v => <option key={v}>{v}</option>)}</select></label>
      <Button primary type="submit" disabled={mutation.busy || !draft.chat}>Save report</Button>
    </form>
  </Dialog>;
}
