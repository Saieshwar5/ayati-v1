import { useRef, useState, type FormEvent } from 'react';
import { Button } from '../ui';
import { day, kindName, kinds, reasons, strengthWord, type MemoryKind, type Note, type NoteChange, type NoteDraft, type Topic } from './memory-model';

/** Five steps and a word: how strong a note is now. */
export function Strength({ value }: { value: number }) {
  const filled = Math.max(0, Math.min(5, Math.ceil(value / 20)));
  return <span className="memory-strength" title={`Strength ${value} of 100`}>
    <span aria-hidden="true">{[0, 1, 2, 3, 4].map(step => <i key={step} className={step < filled ? 'on' : undefined} />)}</span>
    {strengthWord(value)}</span>;
}

/** Where a note came from, in one line. */
function origin(note: Note): string {
  const who = note.source === 'admin' ? `${note.by ?? 'A person'} wrote it` : note.source === 'tidy' ? 'Merged by the tidy-up' : `${note.by ?? 'An agent'} learned it`;
  const where = note.room ? ` in ${note.room}` : note.learned_in ? ` in ${note.learned_in.name}` : note.source === 'agent' ? ' in a conversation you can’t open' : '';
  const used = note.used ? ` · used ${note.used === 1 ? 'once' : `${note.used} times`}` : '';
  const until = note.until_ms ? ` · until ${day(note.until_ms)}` : '';
  return `${who}${where} · ${day(note.created_ms)}${used}${until}`;
}

/** One note with the changes a person can make to it. Removing asks first, in place. */
export function NoteRow({ note, busy, change }: { note: Note; busy: boolean; change: (change: NoteChange, done: string) => void }) {
  const [removing, setRemoving] = useState(false);
  const active = note.state === 'active';
  return <li className="memory-note">
    <div className="memory-note-text">
      <p>{note.text}</p>
      <p className="memory-meta">
        <span className="ui-badge">{kindName(note.kind)}</span>
        {note.topic && <span className="ui-badge">{note.topic}</span>}
        {note.pinned && <span className="ui-badge memory-pinned">Pinned</span>}
        {note.essential && <span className="ui-badge">Essential</span>}
        {!active && <span className="ui-badge">{note.state === 'archived' ? 'Archived' : note.state === 'folded' ? 'In the summary' : 'Ended'}</span>}
        <span>{origin(note)}</span>
      </p>
    </div>
    {active && <Strength value={note.strength} />}
    <div className="memory-actions" role="group" aria-label={`Change: ${note.text}`}>
      {removing ? <>
        <span className="memory-confirm">Remove for good?</span>
        <Button disabled={busy} onClick={() => change('remove', 'Removed.')}>Remove</Button>
        <Button disabled={busy} onClick={() => setRemoving(false)}>Keep it</Button>
      </> : active ? <>
        <Button disabled={busy} onClick={() => change(note.pinned ? 'unpin' : 'pin', note.pinned ? 'Unpinned.' : 'Pinned: it never fades and comes first.')}>{note.pinned ? 'Unpin' : 'Pin'}</Button>
        <Button disabled={busy} onClick={() => change('confirm', 'Confirmed.')}>Confirm</Button>
        <Button disabled={busy} onClick={() => change(note.essential ? 'unessential' : 'essential', note.essential ? 'No longer an essential.' : 'Every agent now gets it with every request.')}>{note.essential ? 'Not essential' : 'Make essential'}</Button>
        <Button disabled={busy} onClick={() => change('end', 'Ended: agents no longer use it.')}>No longer true</Button>
        <Button disabled={busy} onClick={() => setRemoving(true)}>Remove…</Button>
      </> : <>
        <Button disabled={busy} onClick={() => change('restore', 'Brought back.')}>Bring back</Button>
        <Button disabled={busy} onClick={() => setRemoving(true)}>Remove…</Button>
      </>}
    </div>
  </li>;
}

/** A note waiting for a person: kept, or discarded. A correction shows the note it replaces. */
export function WaitingNote({ note, busy, change }: { note: Note; busy: boolean; change: (change: NoteChange, done: string) => void }) {
  return <article className="memory-waiting" aria-label={note.text}>
    <h4>{reasons[note.waiting_reason ?? ''] ?? 'Waits for an answer'}</h4>
    {note.replaces && <p className="memory-quote"><small>Now in memory{note.replaces.used ? `, used ${note.replaces.used} times` : ''}</small>{note.replaces.text}</p>}
    <p className="memory-quote"><small>{note.replaces ? 'Proposed instead' : 'Proposed'} · {origin(note)}</small>{note.text}</p>
    <div className="memory-actions">
      <Button primary disabled={busy} onClick={() => change('keep', note.replaces ? 'Kept, and the old note ended.' : 'Kept: agents use it now.')}>{note.replaces ? 'Replace it' : 'Keep'}</Button>
      <Button disabled={busy} onClick={() => change('discard', 'Discarded.')}>{note.replaces ? 'Keep the old note' : 'Discard'}</Button>
    </div>
  </article>;
}

/** One topic: its summary (or strongest note), how much it is used, and its summary editor. */
export function TopicRow({ topic, busy, open, save }: {
  topic: Topic; busy: boolean; open: () => void; save: (summary: string, done: () => void) => void;
}) {
  const [draft, setDraft] = useState<string>();
  return <li className="memory-topic">
    <button type="button" className="memory-topic-open" onClick={open} aria-label={`Open ${topic.topic}`}>
      <strong>{topic.topic}</strong>
      <span>{topic.summary ?? topic.lead}</span>
      <small>{topic.notes === 1 ? '1 note' : `${topic.notes} notes`} · used {topic.used} times · updated {day(topic.updated_ms)}</small>
    </button>
    <Strength value={topic.strength} />
    {draft === undefined
      ? <Button disabled={busy} onClick={() => setDraft(topic.summary ?? '')}>{topic.summary ? 'Edit summary' : 'Write a summary'}</Button>
      : <form className="memory-summary-form" onSubmit={(event: FormEvent) => { event.preventDefault(); save(draft.trim(), () => setDraft(undefined)); }}>
        <label className="ui-field">Summary of {topic.topic}<textarea maxLength={1200} rows={4} value={draft} onChange={event => setDraft(event.target.value)} /></label>
        <div className="memory-actions"><Button type="submit" primary disabled={busy}>Save summary</Button><Button disabled={busy} onClick={() => setDraft(undefined)}>Cancel</Button></div>
      </form>}
  </li>;
}

/** Adds one note. A retried save of the same words reuses its id, so it is kept once. */
export function AddNote({ busy, add, close }: { busy: boolean; add: (note: NoteDraft, done: () => void) => void; close: () => void }) {
  const [kind, setKind] = useState<MemoryKind>('customer');
  const [topic, setTopic] = useState('');
  const [text, setText] = useState('');
  const [until, setUntil] = useState('');
  const attempt = useRef<NoteDraft | undefined>(undefined);
  const line = text.replace(/\s+/g, ' ').trim();
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!line) return;
    const next = { kind, topic: topic.trim(), text: line, until: until || null };
    const same = attempt.current && JSON.stringify({ ...attempt.current, id: undefined }) === JSON.stringify({ ...next, id: undefined });
    attempt.current = same ? attempt.current : { id: crypto.randomUUID(), ...next };
    add(attempt.current!, () => { attempt.current = undefined; setText(''); setTopic(''); setUntil(''); });
  };
  return <form className="memory-add" onSubmit={submit} aria-label="Add a note">
    <label className="ui-field">Note<input maxLength={300} value={text} onChange={event => setText(event.target.value)} placeholder="Acme Traders pays 45 days after invoice." /></label>
    <div className="memory-add-row">
      <label className="ui-field">About<select value={kind} onChange={event => setKind(event.target.value as MemoryKind)}>
        {kinds.map(([id, name]) => <option key={id} value={id}>{name}</option>)}</select></label>
      <label className="ui-field">Topic<input maxLength={80} value={topic} onChange={event => setTopic(event.target.value)} placeholder="Acme Traders" /></label>
      <label className="ui-field">True until<input type="date" value={until} onChange={event => setUntil(event.target.value)} /></label>
    </div>
    <div className="memory-actions"><Button type="submit" primary disabled={busy || !line}>Save note</Button><Button disabled={busy} onClick={close}>Cancel</Button></div>
  </form>;
}
