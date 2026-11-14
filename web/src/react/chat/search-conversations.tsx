import { useRef, useState, type KeyboardEvent } from 'react';
import { ConversationFace, ParticipantFace, ParticipantName } from './conversation-face';
import { displayAgentName } from '../../agents';
import { workspaceSession } from '../../workspace-context';
import { Button, Dialog } from '../ui';
import { existingDirect, useDirectConversation, type DirectTarget } from './direct-conversation';
import { conversationName, type Conversation, type Directory } from './model';
import { MessageResults, searchable } from './message-search';
import { clearSearches, recentSearches } from './search-query';
import './search-conversations.css';

export function SearchConversations({ directory, chats, close, opened, openMessage }: {
  directory: Directory;
  chats: Conversation[];
  close: () => void;
  opened: (id: string) => void;
  /** Opens a message found by message search in its conversation. */
  openMessage: (chat: string, message: string) => void;
}) {
  const [query, setQuery] = useState('');
  const [category, setCategory] = useState('All');
  const [recent, setRecent] = useState(recentSearches);
  const input = useRef<HTMLInputElement>(null);
  const results = useRef<HTMLUListElement>(null);
  const direct = useDirectConversation(chats, opened);
  const matches = (name: string) => name.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase());
  const channels = chats.filter(chat => chat.kind !== 'direct' && matches(conversationName(chat, directory)));
  const people: DirectTarget[] = directory.users
    .filter(user => user.id !== workspaceSession?.user?.id)
    .map(user => ({ ...user, kind: 'user' }));
  const agents: DirectTarget[] = directory.agents
    .map(agent => ({ ...agent, name: displayAgentName(agent), kind: 'agent' }));
  const targets = [
    ...(category === 'All' || category === 'People' ? people : []),
    ...(category === 'All' || category === 'Agents' ? agents : []),
  ].filter(target => matches(`${target.name} ${target.email ?? ''}`));
  const showChannels = category === 'All' || category === 'Channels';
  const onlyMessages = category === 'Messages';
  const showMessages = onlyMessages || (category === 'All' && searchable(query));
  const count = onlyMessages ? 0 : targets.length + (showChannels ? channels.length : 0);

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

  return <Dialog title="Search" busy={direct.busy} status={direct.status}
    close={close} initialFocus={input}>
    <div className="chat-search" onKeyDown={move}>
      <label className="ui-field">Conversations, people, agents and messages
        <input ref={input} type="search" value={query} onChange={event => setQuery(event.target.value)}
          placeholder="Search…" />
      </label>
      <div className="chat-search-filters" role="group" aria-label="Search category">
        {['All', 'Channels', 'People', 'Agents', 'Messages'].map(value =>
          <Button key={value} aria-pressed={category === value} onClick={() => setCategory(value)}>
            {value}
          </Button>)}
      </div>
      {!onlyMessages && <p className="ui-help" role="status">
        {count ? `${count} ${count === 1 ? 'result' : 'results'}` : 'No matching conversations, people or agents.'}
      </p>}
      {!onlyMessages && <ul className="chat-search-results" ref={results}>
        {showChannels && channels.map(chat => <li key={`chat:${chat.id}`}>
          <button type="button" className="chat-contact" onClick={() => opened(chat.id)}
            aria-label={`Open channel ${chat.name}`}>
            <ConversationFace chat={chat} directory={directory} />
            <span>{chat.name}<small>{chat.archived ? 'Archived channel' : 'Channel'}</small></span>
          </button>
        </li>)}
        {targets.map(target => {
          const existing = existingDirect(chats, target, workspaceSession?.user?.id);
          const action = existing ? 'Open conversation with' : 'Start conversation with';
          return <li key={`${target.kind}:${target.id}`}>
            <button type="button" className="chat-contact" onClick={() => void direct.choose(target)}
              aria-label={`${action} ${target.name}`}>
              <ParticipantFace kind={target.kind} who={target} />
              <span><ParticipantName kind={target.kind} name={target.name} /><small>
                {(target.kind === 'agent' ? target.role : target.title) || (target.kind === 'agent' ? 'Agent' : 'Person')} · {existing ? 'Open conversation' : 'Start conversation'}
              </small></span>
            </button>
          </li>;
        })}
      </ul>}
      {!query.trim() && recent.length > 0 && <div className="chat-search-recent" role="group" aria-label="Recent searches">
        <span>Recent searches</span>
        {recent.map(item => <Button key={item} className="chat-search-chip" onClick={() => { setQuery(item); setCategory('Messages'); input.current?.focus(); }}>{item}</Button>)}
        <button type="button" className="chat-thread-link" onClick={() => { clearSearches(); setRecent([]); }}>Clear</button>
      </div>}
      {showMessages && <><h3 className="chat-search-heading">Messages</h3>
        <MessageResults query={query} chats={chats} directory={directory} open={openMessage} setQuery={setQuery} /></>}
    </div>
  </Dialog>;
}
