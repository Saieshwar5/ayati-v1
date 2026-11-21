import { useId, useState, type RefObject } from 'react';
import { AiTag } from '../faces';
import { agentStateText, takesMessages } from './agent-state';
import { ParticipantFace } from './conversation-face';
import type { Participant } from './model';

export type ForwardTarget = Participant & { kind: 'user' | 'agent' };

/** How many are offered before any typing, and how many matches a search shows. */
const OFFERED = 4, MATCHES = 6;

const line = (item: ForwardTarget) => (item.kind === 'agent' ? item.role : item.title) ?? '';
/** A paused agent takes no new messages, so it is shown and cannot be chosen. */
const open = (item: ForwardTarget) => item.kind === 'user' || takesMessages(item);

/**
 * Who a forward goes to: one field. Until someone is chosen it searches, with the
 * most recent conversations offered first; ↑ ↓ move and Enter chooses. Once chosen
 * the list closes and the choice is a chip.
 */
export function ForwardRecipient({ candidates, recent, target, choose, input }: {
  /** Everyone a forward can go to, most recent conversation first. */
  candidates: ForwardTarget[];
  /** Whether that order comes from the person's own conversations. */
  recent: boolean;
  target?: ForwardTarget; choose: (target?: ForwardTarget) => void;
  input: RefObject<HTMLInputElement | null>;
}) {
  const id = useId();
  const [query, setQuery] = useState('');
  const [active, setActive] = useState(0);
  if (target) return <div className="chat-forward-to">
    <span className="chat-forward-to-label">To</span>
    <span className="chat-forward-chip">
      <ParticipantFace kind={target.kind} who={target} /><span className="chat-forward-chip-name">{target.name}</span>{target.kind === 'agent' && <AiTag />}
      <button type="button" aria-label={`Choose someone other than ${target.name}`} title="Choose someone else" onClick={() => choose(undefined)}>×</button>
    </span>
  </div>;
  const words = query.trim().toLowerCase();
  const rows = words ? candidates.filter(item => `${item.name} ${line(item)}`.toLowerCase().includes(words)).slice(0, MATCHES) : candidates.slice(0, OFFERED);
  const current = Math.min(active, Math.max(rows.length - 1, 0));
  const pick = (item: ForwardTarget | undefined) => {
    if (!item || !open(item)) return;
    setQuery(''); setActive(0); choose(item);
  };
  return <>
    <label className="chat-forward-to">
      <span className="chat-forward-to-label">To</span>
      {/* Remounted when a choice is cleared, so focus returns here. */}
      <input ref={input} autoFocus type="text" role="combobox" aria-label="Forward to" aria-expanded="true" aria-autocomplete="list"
        aria-controls={`${id}-list`} aria-activedescendant={rows[current] ? `${id}-${current}` : undefined}
        value={query} placeholder="Search people and agents" autoComplete="off"
        onChange={event => { setQuery(event.target.value); setActive(0); }}
        onKeyDown={event => {
          if (event.nativeEvent.isComposing) return;
          if ((event.key === 'ArrowDown' || event.key === 'ArrowUp') && rows.length) {
            event.preventDefault();
            setActive((current + (event.key === 'ArrowDown' ? 1 : rows.length - 1)) % rows.length);
          } else if (event.key === 'Enter') { event.preventDefault(); pick(rows[current]); }
        }} />
    </label>
    <div className="chat-forward-list" id={`${id}-list`} role="listbox" aria-label="People and agents">
      {!words && rows.length > 0 && <p className="chat-forward-list-label">{recent ? 'Recent' : 'People and agents'}</p>}
      {rows.map((item, index) => <button type="button" role="option" tabIndex={-1} id={`${id}-${index}`} key={`${item.kind}:${item.id}`}
        className="chat-forward-row" aria-selected={index === current} aria-disabled={!open(item)} onClick={() => pick(item)}>
        <ParticipantFace kind={item.kind} who={item} /><span className="chat-forward-row-name">{item.name}</span>{item.kind === 'agent' && <AiTag />}
        <span className="chat-forward-row-line">{line(item)}</span>
        {!open(item) && <span className="chat-forward-row-state">{agentStateText(item).label}</span>}
      </button>)}
      {!rows.length && <p className="chat-help">No one matches “{query.trim()}”.</p>}
      {!words && candidates.length > rows.length && <p className="chat-forward-list-more">Type to search all {candidates.length}.</p>}
    </div>
  </>;
}
