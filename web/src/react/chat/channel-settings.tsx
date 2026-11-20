import { useId, useState, type ReactNode } from 'react';
import { useMutation } from '../requests';
import { Button } from '../ui';
import { LockIcon } from './conversation-face';
import { Icon } from './panel-actions';
import { chatRequest, type Conversation } from './model';

type Replies = 'thread' | 'channel';
type Values = { name: string; description: string; visibility: 'public' | 'private'; announcements: boolean; replies: Replies };
const valuesOf = (chat: Conversation): Values => ({ name: chat.name, description: chat.description ?? '', visibility: chat.visibility ?? 'private', announcements: Boolean(chat.announcements), replies: chat.agent_replies ?? 'thread' });
const same = (a: Values, b: Values) => a.name === b.name && a.description === b.description && a.visibility === b.visibility && a.announcements === b.announcements && a.replies === b.replies;
const changes = (draft: Values, saved: Values) => [draft.name.trim() !== saved.name.trim(), draft.description.trim() !== saved.description.trim(),
  draft.visibility !== saved.visibility, draft.announcements !== saved.announcements, draft.replies !== saved.replies].filter(Boolean).length;

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
      ...(draft.replies !== saved.replies ? { agent_replies: draft.replies } : {}),
    });
    setDone(true);
    await changed();
  });
  // Only what has consequences is said in words: opening the history, or locking yourself out of posting.
  const opening = draft.visibility === 'public' && saved.visibility === 'private';
  return <section className="chat-details-section chat-settings" aria-label={`#${chat.name} settings`} aria-busy={mutation.busy}>
    <form className="ui-form chat-details-form" onSubmit={event => { event.preventDefault(); if (unsaved) save(); }}>
      <label className="ui-field">Name
        <input required maxLength={100} value={draft.name} onChange={event => edit({ name: event.target.value })} />
      </label>
      <label className="ui-field">Description
        <textarea rows={2} maxLength={300} value={draft.description} placeholder="What this channel is for"
          onChange={event => edit({ description: event.target.value })} />
      </label>
      <Choice legend="Who can post" name={`${id}-post`} value={draft.announcements ? 'admins' : 'everyone'} change={value => edit({ announcements: value === 'admins' })}
        options={[{ value: 'everyone', label: 'Everyone' }, { value: 'admins', label: 'Only admins' }]}
        note={draft.announcements && !admin ? 'You are not an admin, so you could not post either.' : undefined} />
      <Choice legend="Agents answer" name={`${id}-replies`} value={draft.replies} change={value => edit({ replies: value as Replies })}
        options={[{ value: 'thread', label: 'In a thread', icon: <Icon name="thread" /> }, { value: 'channel', label: 'In the channel', icon: <Icon name="message" /> }]} />
      <Choice legend="Who can find it" name={`${id}-visibility`} value={admin ? draft.visibility : saved.visibility} disabled={!admin}
        change={value => edit({ visibility: value as Values['visibility'] })}
        options={[{ value: 'private', label: 'Private', icon: <LockIcon size={14} /> }, { value: 'public', label: 'Public', icon: <Icon name="hash" /> }]}
        note={!admin ? 'Only admins change this' : opening ? 'Everyone in the workspace will be able to read its history and files.' : undefined} />
      <div className="chat-details-save">
        <Button primary type="submit" disabled={mutation.busy || !unsaved}>{mutation.busy ? 'Saving…' : 'Save changes'}</Button>
        {unsaved > 0 && <small>{unsaved} unsaved {unsaved === 1 ? 'change' : 'changes'}</small>}
        <small role="status">{done && !unsaved ? 'Saved.' : ''}</small>
      </div>
      {mutation.status && <p className="chat-error" role="alert">{mutation.status}</p>}
    </form>
  </section>;
}

/** Two or three choices side by side: real radio buttons, drawn as one switch. */
function Choice({ legend, name, value, options, change, disabled = false, note }: {
  legend: string; name: string; value: string; change: (value: string) => void; disabled?: boolean; note?: string;
  options: { value: string; label: string; icon?: ReactNode }[];
}) {
  return <fieldset className="chat-details-choice chat-choice-switch" disabled={disabled}>
    <legend>{legend}</legend>
    <div>{options.map(option => <label key={option.value}>
      <input type="radio" name={name} value={option.value} checked={value === option.value} onChange={() => change(option.value)} />
      {option.icon}{option.label}
    </label>)}</div>
    {note && <p className="ui-help">{disabled && <LockIcon size={12} />}{note}</p>}
  </fieldset>;
}
