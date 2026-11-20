import { useEffect, useRef, useState } from 'react';
import { Button } from '../ui';
import { ParticipantFace, ParticipantName } from './conversation-face';
import type { Conversation, Directory, Participant } from './model';
import { teamAgent } from './start-model';

type Kind = 'user' | 'agent';

/** Add people and agents to a channel from one list (2026-11-20; before, a button each); each Add saves at once. */
export function ChannelAdd({ chat, directory, users, agents, busy, add, done }: {
  chat: Conversation; directory: Directory; users: string[]; agents: string[]; busy: boolean;
  add: (kind: Kind, who: Participant) => Promise<void>; done: () => void;
}) {
  const [query, setQuery] = useState('');
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => { input.current?.focus(); }, []);
  // Assistants and older "Only me" agents cannot join channels; the server refuses them too.
  const byName = (a: { who: Participant }, b: { who: Participant }) => a.who.name.localeCompare(b.who.name);
  const candidates = [
    ...directory.users.filter(user => !users.includes(user.id)).map(who => ({ kind: 'user' as const, who })).sort(byName),
    ...directory.agents.filter(agent => teamAgent(agent) && !agents.includes(agent.id)).map(who => ({ kind: 'agent' as const, who })).sort(byName),
  ];
  const text = query.trim().toLowerCase();
  const shown = candidates.filter(({ who }) => `${who.name} ${who.title ?? ''} ${who.role ?? ''} ${who.email ?? ''}`.toLowerCase().includes(text));
  const title = `Add people or agents to #${chat.name}`;
  return <div className="chat-details-adding" role="group" aria-label={title}
    onKeyDown={event => { if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); done(); } }}>
    <input ref={input} type="search" className="chat-details-search" aria-label="Search people and agents to add"
      placeholder="Search people and agents" value={query} onChange={event => setQuery(event.target.value)} />
    {shown.length > 0 && <ul className="chat-details-members" aria-label={title}>
      {shown.map(({ kind, who }) => <li key={`${kind}:${who.id}`} className="chat-details-member">
        <span className="chat-details-who">
          <ParticipantFace kind={kind} who={who} presence />
          <span><ParticipantName kind={kind} name={who.name} />{(kind === 'agent' ? who.role : who.title) && <small>{kind === 'agent' ? who.role : who.title}</small>}</span>
        </span>
        <Button aria-label={`Add ${who.name}`} disabled={busy} onClick={() => void add(kind, who).then(() => input.current?.focus())}>Add</Button>
      </li>)}
    </ul>}
    {!candidates.length && <p className="chat-help">Everyone who can join is already here.</p>}
    {candidates.length > 0 && !shown.length && <p className="chat-help" role="status">Nobody matches “{query.trim()}”.</p>}
    <Button className="chat-details-done" onClick={done}>Done</Button>
  </div>;
}
