import { useState } from 'react';
import { chatRequest } from '../chat/model';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
const loadGeneral = () => chatRequest<{ name: string }>('/settings');
export function WorkspaceGeneral({ renamed }: { renamed: (name: string) => void }) {
  const resource = useResource(loadGeneral);
  return <section><h2>Workspace</h2><p>One workspace for working on your own or with others.</p>
    {resource.error ? <p role="alert">{resource.error}</p> : resource.data ? <GeneralForm initial={resource.data.name} renamed={renamed} /> : <p role="status">Loading workspace…</p>}
  </section>;
}
function GeneralForm({ initial, renamed }: { initial: string; renamed: (name: string) => void }) {
  const [name, setName] = useState(initial);
  const [saved, setSaved] = useState(initial);
  const mutation = useMutation();
  return <form className="ui-form settings-general" onSubmit={event => { event.preventDefault(); void mutation.run(async () => {
    const result = await chatRequest<{ name: string }>('/settings', { name, previous_name: saved });
    setSaved(result.name); setName(result.name); renamed(result.name); mutation.setStatus('Workspace name saved.');
  }); }}><fieldset className="ui-fieldset" disabled={mutation.busy}><label className="ui-field">Workspace name<input required maxLength={80} value={name} onChange={event => setName(event.target.value)} /></label>
    <p className="ui-help">Admins manage this workspace. Members use what is shared with them.</p><Button type="submit" primary disabled={mutation.busy}>{mutation.busy ? 'Saving…' : 'Save changes'}</Button></fieldset><p role="status">{mutation.status}</p></form>;
}
type Activity = { id: number; actor: string; action: string; target: string; summary?: string | null; at: number };
const loadActivity = () => chatRequest<{ activity: Activity[] }>('/activity');
export function WorkspaceActivity() {
  const resource = useResource(loadActivity);
  return <section><header className="settings-section-heading"><div><h2>Audit log</h2><p>The latest 100 changes to people, access and settings.</p></div><Button onClick={() => void resource.refresh().catch(() => {})}>Refresh</Button></header>
    {resource.error && <p role="alert">{resource.error}</p>}
    {!resource.error && resource.data?.activity.map(item => <article className="settings-activity" key={item.id}><div>{item.summary ? <span>{item.summary}</span> : <><strong>{item.actor}</strong><span>{item.action.replaceAll('_', ' ')}</span><small>{item.target}</small></>}</div><time dateTime={new Date(item.at * 1000).toISOString()}>{new Date(item.at * 1000).toLocaleString()}</time></article>)}
    {!resource.error && !resource.data && <p role="status">Loading activity…</p>}
    {resource.data?.activity.length === 0 && <p>No workspace changes yet.</p>}
  </section>;
}
