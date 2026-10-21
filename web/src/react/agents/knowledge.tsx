import { useCallback, useState } from 'react';
import { identityRequest } from '../../workspace-context';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { chatRequest, type Conversation } from '../chat/model';
import { base, type Agent } from './model';
const loadConversations = () => chatRequest<{ chats: Conversation[] }>('/chats');
type Note = { id: string; title: string; content: string; chat: string | null; source_message: string | null; revision: number; forgotten: boolean };
export function Knowledge({ agent }: { agent: Agent }) {
  const path = `${base}/${agent.id}/knowledge`;
  const load = useCallback(() => identityRequest<{ notes: Note[] }>(path), [path]);
  const { data, error, refresh } = useResource(load);
  const [draft, edit] = useState<Note>();
  const mutation = useMutation();
  const conversations = useResource(loadConversations);
  async function save(note: Note) { await identityRequest(path, note); edit(undefined); await refresh(); }
  return <section className="agent-knowledge"><h3>Approved knowledge</h3><p className="ui-help">Agent-wide notes can be used in every conversation with this agent. Never publish private information here for a workspace agent.</p>
    <p role="status">{error || mutation.status}</p>
    {data?.notes.map(note => <article key={note.id}><strong>{note.title}</strong><small>{note.chat ? 'Conversation only' : 'Agent-wide'}</small><p>{note.content}</p>{agent.editable && <><Button disabled={mutation.busy} onClick={() => edit(note)}>Edit</Button><Button disabled={mutation.busy} onClick={() => { if (confirm('Stop using this note? The audit record is retained.')) void mutation.run(() => save({ ...note, forgotten: true })); }}>Forget</Button></>}</article>)}
    {data && !data.notes.length && <p className="ui-help">No approved notes yet.</p>}
    {agent.editable && !draft && <Button onClick={() => edit({ id: crypto.randomUUID(), title: '', content: '', chat: null, source_message: null, revision: 0, forgotten: false })}>Add approved note</Button>}
    {draft && <form className="ui-form" onSubmit={e => { e.preventDefault(); void mutation.run(() => save(draft)); }}><label className="ui-field">Where can this note be used?<select disabled={Boolean(data?.notes.some(n => n.id === draft.id))} value={draft.chat ?? ''} onChange={e => edit({ ...draft, chat: e.target.value || null })}><option value="">Every conversation with this agent</option>{conversations.data?.chats.filter(c => c.agents.includes(agent.id)).map(c => <option key={c.id} value={c.id}>{c.name} · this conversation only</option>)}</select></label><label className="ui-field">Title<input required maxLength={160} value={draft.title} onChange={e => edit({ ...draft, title: e.target.value })} /></label><label className="ui-field">Note<textarea required maxLength={12000} rows={4} value={draft.content} onChange={e => edit({ ...draft, content: e.target.value })} /></label><Button type="submit" disabled={mutation.busy} primary>Save note</Button><Button disabled={mutation.busy} onClick={() => edit(undefined)}>Cancel</Button></form>}
  </section>;
}
