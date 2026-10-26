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
};
type Draft = { note: Note; audience: string; existing: boolean };

export function Knowledge({ agent }: { agent: Agent }) {
  const path = `${base}/${agent.id}/knowledge`;
  const load = useCallback(() => identityRequest<{ notes: Note[] }>(path), [path]);
  const { data, error, refresh } = useResource(load);
  const [draft, edit] = useState<Draft>();
  const [preview, showPreview] = useState(false);
  const mutation = useMutation();
  const conversations = useResource(loadConversations);
  const eligible = conversations.data?.chats?.filter(chat => chat.agents.includes(agent.id)) ?? [];

  async function save(note: Note) {
    // Read records also carry server metadata; send only the mutation contract.
    const { id, title, content, chat, source_message, revision, forgotten } = note;
    await identityRequest(path, { id, title, content, chat, source_message, revision, forgotten });
    edit(undefined);
    await refresh();
  }
  function changeNote(change: Partial<Note>) {
    edit(current => current && { ...current, note: { ...current.note, ...change } });
  }
  function add() {
    mutation.setStatus('');
    edit({ audience: '', existing: false, note: {
      id: crypto.randomUUID(), title: '', content: '', chat: null,
      source_message: null, revision: 0, forgotten: false,
    } });
  }
  return <section className="agent-knowledge">
    <section className="agent-context-description" aria-label="Conversation context">
      <h3>Conversation context</h3>
      <p className="ui-help">Each conversation keeps its own history. A channel uses its shared messages; private chats are kept separate.</p>
      <p className="ui-help">{agent.settings.history === 'since_join'
        ? 'Channel history: only messages since this agent joined.'
        : 'Channel history can include messages from before this agent joined.'}</p>
      <Button onClick={() => showPreview(true)}>Preview conversation context</Button>
    </section>
    {preview && <ContextPreviewDialog agent={agent.id} conversations={eligible}
      conversationError={conversations.error} close={() => showPreview(false)} />}
    <h3>Approved knowledge</h3>
    <p className="ui-help">You choose where each note can be used. A note for every conversation can reach anyone who can talk to this agent. Notes are not learned automatically from private chats.</p>
    {!agent.settings.memory && <p className="ui-help">Approved knowledge is off. Notes stay saved; new work will not use them until enabled in the agent profile.</p>}
    <p role="status">{error || mutation.status}</p>
    {!draft && <>
      {data?.notes.map(note => <article key={note.id}>
        <strong>{note.title}</strong>
        <small>{note.chat ? 'Conversation only' : 'Every conversation with this agent'}</small>
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
      {agent.editable && data && <Button disabled={mutation.busy} onClick={add}>Add approved note</Button>}
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
