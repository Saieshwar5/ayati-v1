import { useCallback, useState } from 'react';
import { identityRequest } from '../../workspace-context';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { chatRequest, type Conversation } from '../chat/model';
import { base, type Agent } from './model';
import { ContextPreviewDialog } from './context-preview';

const loadConversations = () => chatRequest<{ chats: Conversation[] }>('/chats');
type Note = {
  id: string; title: string; content: string; chat: string | null;
  source_message: string | null; revision: number; forgotten: boolean;
  /** The lesson this note was approved from (schema 121). */
  learned_from?: string | null;
};
type Draft = { note: Note; audience: string; existing: boolean };

/** Opens the preview of what the agent is given from one conversation. The
 * conversations are read when the button is first used. `quiet` is the page's
 * small link style. */
export function ContextPreviewButton({ agent, quiet = false }: { agent: Agent; quiet?: boolean }) {
  const [open, setOpen] = useState(false);
  return <>
    {quiet ? <button type="button" className="profile-lock" aria-label="Preview conversation context" onClick={() => setOpen(true)}>Conversation context</button>
      : <Button onClick={() => setOpen(true)}>Preview conversation context</Button>}
    {open && <ContextPreview agent={agent} close={() => setOpen(false)} />}
  </>;
}
function ContextPreview({ agent, close }: { agent: Agent; close: () => void }) {
  const conversations = useResource(loadConversations);
  return <ContextPreviewDialog agent={agent.id} conversations={conversations.data?.chats?.filter(chat => chat.agents.includes(agent.id)) ?? []}
    conversationError={conversations.error} close={close} />;
}

/** An agent's approved notes. `adding` opens the new-note form at once (the
 * page's ＋ Note); `changed` is told after a note is saved or forgotten. Lessons
 * from its work have their own panel (learned-lessons.tsx). */
export function Knowledge({ agent, adding = false, changed }: { agent: Agent; adding?: boolean; changed?: () => void }) {
  const path = `${base}/${agent.id}/knowledge`;
  const load = useCallback(() => identityRequest<{ notes: Note[] }>(path), [path]);
  const { data, error, refresh } = useResource(load);
  const blank = (): Draft => ({ audience: '', existing: false, note: { id: crypto.randomUUID(), title: '', content: '', chat: null, source_message: null, revision: 0, forgotten: false } });
  const [draft, edit] = useState<Draft | undefined>(() => adding && agent.editable ? blank() : undefined);
  const mutation = useMutation();
  const conversations = useResource(loadConversations);
  const eligible = conversations.data?.chats?.filter(chat => chat.agents.includes(agent.id)) ?? [];

  async function save(note: Note) {
    // Read records also carry server metadata; send only the mutation contract.
    const { id, title, content, chat, source_message, revision, forgotten } = note;
    await identityRequest(path, { id, title, content, chat, source_message, revision, forgotten });
    edit(undefined);
    await refresh();
    changed?.();
  }
  function changeNote(change: Partial<Note>) {
    edit(current => current && { ...current, note: { ...current.note, ...change } });
  }
  function add() {
    mutation.setStatus('');
    edit(blank());
  }
  return <section className="agent-knowledge">
    <p role="status">{error || mutation.status}</p>
    {!draft && <>
      {data?.notes.map(note => <article key={note.id}>
        <strong>{note.title}</strong>
        <small>{note.chat ? 'Conversation only' : 'Every conversation with this agent'}{note.learned_from ? ' · Learned from its work' : ''}</small>
        <p>{note.content}</p>
        {agent.editable && <>
          <Button disabled={mutation.busy} onClick={() => {
            mutation.setStatus('');
            edit({ note, audience: note.chat ?? 'agent', existing: true });
          }}>Edit</Button>
          <Button disabled={mutation.busy} onClick={() => {
            if (confirm('Stop retrieving this approved note? Existing conversation messages and the audit record are retained.'))
              void mutation.run(() => save({ ...note, forgotten: true }));
          }}>Forget</Button>
        </>}
      </article>)}
      {data && !data.notes.length && <p className="ui-help">No approved notes yet.</p>}
      <div className="agent-knowledge-actions">
        {agent.editable && data && <Button disabled={mutation.busy} onClick={add}>Add approved note</Button>}
        <ContextPreviewButton agent={agent} />
      </div>
    </>}
    {draft && agent.editable && <form className="ui-form" aria-label={draft.existing ? 'Edit approved note' : 'Add approved note'}
      onSubmit={event => {
        event.preventDefault();
        if (!draft.audience) {
          mutation.setStatus('Choose where this note can be used before saving.');
          return;
        }
        void mutation.run(() => save({ ...draft.note, chat: draft.audience === 'agent' ? null : draft.audience }));
      }}>
      <label className="ui-field">Where can this note be used?
        <select autoFocus required disabled={draft.existing || mutation.busy} value={draft.audience}
          onChange={event => edit({ ...draft, audience: event.target.value })}>
          <option value="" disabled>Choose an audience…</option>
          {eligible.map(chat => <option key={chat.id} value={chat.id}>{chat.name} · this conversation only</option>)}
          {draft.existing && draft.note.chat && !eligible.some(chat => chat.id === draft.note.chat) &&
            <option value={draft.note.chat}>Saved conversation</option>}
          <option value="agent">Every conversation with this agent</option>
        </select>
      </label>
      <p className="ui-help">{draft.existing
        ? 'This note keeps its original audience. Create a separate reviewed note to use a different audience.'
        : draft.audience === 'agent'
          ? 'This note may be used with anyone who can talk to this agent. Do not include private employee or customer details.'
          : draft.audience
            ? 'This note is limited to the selected conversation. It does not grant new access or permission.'
            : 'Choose one conversation or explicitly allow every conversation with this agent.'}</p>
      {conversations.error && <p role="alert">Conversation choices are unavailable: {conversations.error}</p>}
      <label className="ui-field">Title
        <input required disabled={mutation.busy} maxLength={160} value={draft.note.title}
          onChange={event => changeNote({ title: event.target.value })} />
      </label>
      <label className="ui-field">Note
        <textarea required disabled={mutation.busy} maxLength={12000} rows={4} value={draft.note.content}
          onChange={event => changeNote({ content: event.target.value })} />
      </label>
      <div className="agent-knowledge-actions">
        <Button type="submit" disabled={mutation.busy || !draft.audience} primary>{mutation.busy ? 'Saving…' : 'Save note'}</Button>
        <Button disabled={mutation.busy} onClick={() => edit(undefined)}>Cancel</Button>
      </div>
    </form>}
  </section>;
}
