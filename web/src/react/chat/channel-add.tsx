import { useEffect, useRef, useState } from 'react';
import { Button } from '../ui';
import { ParticipantFace, ParticipantName } from './conversation-face';
import type { Conversation, Directory, Participant } from './model';

/** Add people or an agent to a channel from the workspace directory; each Add saves at once. */
export function ChannelAdd({ kind, chat, directory, present, busy, add, done }: {
  kind: 'user' | 'agent'; chat: Conversation; directory: Directory; present: string[]; busy: boolean;
  add: (who: Participant) => Promise<void>; done: () => void;
}) {
  const [query, setQuery] = useState('');
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => { input.current?.focus(); }, []);
  // Agents kept for one person cannot join channels; the server refuses them too.
  const pool = kind === 'user' ? directory.users : directory.agents.filter(agent => agent.audience !== 'only_me');
  const candidates = pool.filter(item => !present.includes(item.id)).sort((a, b) => a.name.localeCompare(b.name));
  const text = query.trim().toLowerCase();
  const shown = candidates.filter(item => `${item.name} ${item.title ?? ''} ${item.role ?? ''} ${item.email ?? ''}`.toLowerCase().includes(text));
  const title = kind === 'user' ? `Add people to #${chat.name}` : `Add an agent to #${chat.name}`;
  return <div className="chat-details-adding" role="group" aria-label={title}
    onKeyDown={event => { if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); done(); } }}>
    <input ref={input} type="search" className="chat-details-search" aria-label={kind === 'user' ? 'Search people to add' : 'Search agents to add'}
      placeholder={kind === 'user' ? 'Search people in the workspace' : 'Search shared agents'} value={query} onChange={event => setQuery(event.target.value)} />
    {shown.length > 0 && <ul className="chat-details-members" aria-label={title}>
      {shown.map(item => <li key={item.id} className="chat-details-member">
        <span className="chat-details-who">
          <ParticipantFace kind={kind} who={item} />
          <span><ParticipantName kind={kind} name={item.name} />{(kind === 'agent' ? item.role : item.title) && <small>{kind === 'agent' ? item.role : item.title}</small>}</span>
        </span>
        <Button aria-label={`Add ${item.name}`} disabled={busy} onClick={() => void add(item).then(() => input.current?.focus())}>Add</Button>
      </li>)}
    </ul>}
    {!candidates.length && <p className="chat-help">{kind === 'user' ? 'Everyone in the workspace is already here.' : 'No other shared agents can join this channel.'}</p>}
    {candidates.length > 0 && !shown.length && <p className="chat-help" role="status">Nobody matches “{query.trim()}”.</p>}
    <Button className="chat-details-done" onClick={done}>Done</Button>
  </div>;
}
