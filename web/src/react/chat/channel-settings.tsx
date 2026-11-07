import { useId, useState } from 'react';
import { useMutation } from '../requests';
import { Button } from '../ui';
import { LockIcon } from './conversation-face';
import { chatRequest, type Conversation } from './model';

type Values = { name: string; description: string; visibility: 'public' | 'private'; announcements: boolean };
const valuesOf = (chat: Conversation): Values => ({ name: chat.name, description: chat.description ?? '', visibility: chat.visibility ?? 'private', announcements: Boolean(chat.announcements) });
const same = (a: Values, b: Values) => a.name === b.name && a.description === b.description && a.visibility === b.visibility && a.announcements === b.announcements;
const changes = (draft: Values, saved: Values) => [draft.name.trim() !== saved.name.trim(), draft.description.trim() !== saved.description.trim(),
  draft.visibility !== saved.visibility, draft.announcements !== saved.announcements].filter(Boolean).length;

/**
 * Name, description, who can find it and who can post, saved together (D245
 * screen 6), for the channel's creator and workspace admins. Only admins change
 * who can find it, because that changes who can read its history; the server
 * enforces the same rule.
 */
export function ChannelSettings({ chat, admin, changed }: { chat: Conversation; admin: boolean; changed: () => Promise<void> }) {
  const id = useId();
  const saved = valuesOf(chat);
  const [draft, setDraft] = useState(saved);
  const [base, setBase] = useState(saved);
  const [done, setDone] = useState(false);
  const mutation = useMutation();
  // A newer saved version replaces the form only while it has no unsaved edits.
  if (!same(base, saved)) { setBase(saved); if (same(draft, base)) setDraft(saved); }
  const unsaved = changes(draft, saved);
  const edit = (next: Partial<Values>) => { setDone(false); setDraft(previous => ({ ...previous, ...next })); };
  const save = () => void mutation.run(async () => {
    setDone(false);
    await chatRequest(`/chats/${chat.id}/settings`, {
      name: draft.name.trim(), description: draft.description.trim(),
      visibility: admin ? draft.visibility : saved.visibility, announcements: draft.announcements,
    });
    setDone(true);
    await changed();
  });
  const visibilityHelp = draft.visibility === 'public'
    ? 'Everyone in the workspace can find and read it, including its existing history and published files.'
    : 'Only people added to it can find and read it.';
  return <section className="chat-details-section" aria-labelledby={`${id}-heading`} aria-busy={mutation.busy}>
    <header className="chat-details-section-header"><h3 id={`${id}-heading`}>Channel settings</h3><small>Creator and admins</small></header>
    <form className="ui-form chat-details-form" onSubmit={event => { event.preventDefault(); if (unsaved) save(); }}>
      <label className="ui-field">Name
        <input required maxLength={100} value={draft.name} onChange={event => edit({ name: event.target.value })} />
      </label>
      <label className="ui-field">Description
        <textarea rows={3} maxLength={300} value={draft.description} placeholder="What this channel is for"
          onChange={event => edit({ description: event.target.value })} />
      </label>
      {admin ? <fieldset className="chat-details-choice">
        <legend>Who can find it</legend>
        <div>
          <label><input type="radio" name={`${id}-visibility`} value="public" checked={draft.visibility === 'public'} onChange={() => edit({ visibility: 'public' })} /><span aria-hidden="true">#</span> Public</label>
          <label><input type="radio" name={`${id}-visibility`} value="private" checked={draft.visibility === 'private'} onChange={() => edit({ visibility: 'private' })} /><LockIcon /> Private</label>
        </div>
        <p className="ui-help">{visibilityHelp}</p>
      </fieldset> : <div className="chat-details-fixed" role="group" aria-labelledby={`${id}-find`}>
        <span id={`${id}-find`}>Who can find it</span>
        <p>{saved.visibility === 'public' ? <span aria-hidden="true">#</span> : <LockIcon size={13} />}{saved.visibility === 'public' ? 'Public' : 'Private'}</p>
        <p className="ui-help">Only workspace admins can change this, because it changes who can read the history.</p>
      </div>}
      <label className="ui-field">Who can post
        <select value={draft.announcements ? 'admins' : 'everyone'} onChange={event => edit({ announcements: event.target.value === 'admins' })}>
          <option value="everyone">Everyone in the channel</option>
          <option value="admins">Only admins</option>
        </select>
        <span className="ui-help">{draft.announcements && !admin
          ? 'You are not a workspace admin, so you could not post either. Agents still reply when asked.'
          : 'Agents still reply when asked.'}</span>
      </label>
      <div className="chat-details-save">
        <Button primary type="submit" disabled={mutation.busy || !unsaved}>{mutation.busy ? 'Saving…' : 'Save changes'}</Button>
        {unsaved > 0 && <small>{unsaved} unsaved {unsaved === 1 ? 'change' : 'changes'}</small>}
        <small role="status">{done && !unsaved ? 'Saved.' : ''}</small>
      </div>
      {mutation.status && <p className="chat-error" role="alert">{mutation.status}</p>}
    </form>
  </section>;
}
