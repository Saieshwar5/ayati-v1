import { useRef, useState, type KeyboardEvent } from 'react';
import { displayAgentName } from '../../agents';
import { workspaceSession } from '../../workspace-context';
import { Button, Dialog } from '../ui';
import { existingDirect, useDirectConversation, type DirectTarget } from './direct-conversation';
import { conversationName, type Conversation, type Directory } from './model';
import './search-conversations.css';

export function SearchConversations({ directory, chats, close, opened, privateAgent }: {
  directory: Directory;
  chats: Conversation[];
  close: () => void;
  opened: (id: string) => void;
  privateAgent: (id: string) => void;
}) {
  const [query, setQuery] = useState('');
  const [category, setCategory] = useState('All');
  const input = useRef<HTMLInputElement>(null);
  const results = useRef<HTMLUListElement>(null);
  const direct = useDirectConversation(directory, chats, opened, privateAgent);
  const matches = (name: string) => name.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase());
  const channels = chats.filter(chat => chat.kind !== 'direct' && matches(conversationName(chat, directory)));
  const people: DirectTarget[] = directory.users
    .filter(user => user.id !== workspaceSession?.user?.id)
    .map(user => ({ ...user, kind: 'user' }));
  const agents: DirectTarget[] = directory.agents
    .filter(agent => agent.shared || directory.role === 'Owner')
    .map(agent => ({ ...agent, name: displayAgentName(agent), kind: 'agent' }));
  const targets = [
    ...(category === 'All' || category === 'People' ? people : []),
    ...(category === 'All' || category === 'Agents' ? agents : []),
  ].filter(target => matches(`${target.name} ${target.email ?? ''}`));
  const showChannels = category === 'All' || category === 'Channels';
  const count = targets.length + (showChannels ? channels.length : 0);

  const move = (event: KeyboardEvent) => {
    if (!['ArrowDown', 'ArrowUp'].includes(event.key)) return;
    const buttons = [...(results.current?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? [])];
    if (!buttons.length) return;
    const index = buttons.indexOf(event.target as HTMLButtonElement);
    if (event.target !== input.current && index < 0) return;
    event.preventDefault();
    if (event.key === 'ArrowUp' && index <= 0) input.current?.focus();
    else buttons[(index + (event.key === 'ArrowDown' ? 1 : -1) + buttons.length) % buttons.length]?.focus();
  };

  return <Dialog title="Search conversations" busy={direct.busy} status={direct.status}
    close={close} initialFocus={input}>
    <div className="chat-search" onKeyDown={move}>
      <label className="ui-field">Channels, people and agents
        <input ref={input} type="search" value={query} onChange={event => setQuery(event.target.value)}
          placeholder="Search conversations…" />
      </label>
      <div className="chat-search-filters" role="group" aria-label="Search category">
        {['All', 'Channels', 'People', 'Agents'].map(value =>
          <Button key={value} aria-pressed={category === value} onClick={() => setCategory(value)}>
            {value}
          </Button>)}
      </div>
      <p className="ui-help" role="status">
        {count ? `${count} ${count === 1 ? 'result' : 'results'}` : 'No matching conversations, people or agents.'}
      </p>
      <ul className="chat-search-results" ref={results}>
        {showChannels && channels.map(chat => <li key={`chat:${chat.id}`}>
          <button type="button" className="chat-contact" onClick={() => opened(chat.id)}
            aria-label={`Open channel ${chat.name}`}>
            <span className="chat-avatar channel">#</span>
            <span>{chat.name}<small>Channel</small></span>
          </button>
        </li>)}
        {targets.map(target => {
          const existing = existingDirect(chats, target, workspaceSession?.user?.id);
          const privateChat = target.kind === 'agent' && !target.shared;
          const action = existing || privateChat ? 'Open conversation with' : 'Start conversation with';
          return <li key={`${target.kind}:${target.id}`}>
            <button type="button" className="chat-contact" onClick={() => void direct.choose(target)}
              aria-label={`${action} ${target.name}`}>
              <span className={`chat-avatar ${target.kind}`}>
                {target.kind === 'agent' ? '✧' : target.name.slice(0, 1).toUpperCase()}
              </span>
              <span>{target.name}<small>
                {target.kind === 'agent' ? 'Agent' : 'Person'} · {existing || privateChat ? 'Open conversation' : 'Start conversation'}
              </small></span>
            </button>
          </li>;
        })}
      </ul>
    </div>
  </Dialog>;
}
