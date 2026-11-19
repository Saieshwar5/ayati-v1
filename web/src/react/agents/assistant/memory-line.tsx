import { useState } from 'react';
import { Button } from '../../ui';
import { kinds, type Memory } from './settings-model';

const sources: Record<Memory['source'], (name: string) => string> = {
  you: () => 'You added this',
  chat: name => `${name} learned this working with you`,
  review: name => `${name} learned this looking back at your work`,
};
const day = (ms: number) => new Date(ms).toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' });

/** One remembered line: its words, where it came from and when, with Edit and Remove. */
export function MemoryLine({ name, memory, busy, save, remove }: {
  name: string; memory: Memory; busy: boolean;
  /** Saves the changed line; `saved` runs once it is kept. */
  save: (next: Pick<Memory, 'kind' | 'text'>, saved: () => void) => void; remove: () => void;
}) {
  const [draft, setDraft] = useState<Pick<Memory, 'kind' | 'text'>>();
  if (draft) {
    const text = draft.text.replace(/\s+/g, ' ').trim();
    return <li className="assistant-memory-editing">
      <select aria-label="What the line is about" value={draft.kind} onChange={event => setDraft({ ...draft, kind: event.target.value as Memory['kind'] })}>
        {kinds.map(([id, title]) => <option key={id} value={id}>{title}</option>)}</select>
      <input aria-label="Remembered line" maxLength={300} value={draft.text} onChange={event => setDraft({ ...draft, text: event.target.value })} />
      <span className="assistant-memory-actions">
        <Button primary disabled={busy || !text} onClick={() => save({ kind: draft.kind, text }, () => setDraft(undefined))}>Save</Button>
        <Button disabled={busy} onClick={() => setDraft(undefined)}>Cancel</Button>
      </span>
    </li>;
  }
  return <li>
    <span>{memory.text}<small className="ui-help">{sources[memory.source](name)} · {day(memory.created_ms)}</small></span>
    <span className="assistant-memory-actions">
      <Button disabled={busy} aria-label={`Edit: ${memory.text}`} onClick={() => setDraft({ kind: memory.kind, text: memory.text })}>Edit</Button>
      <Button disabled={busy} aria-label={`Remove: ${memory.text}`} onClick={remove}>Remove</Button>
    </span>
  </li>;
}
