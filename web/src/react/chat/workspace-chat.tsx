import { useEffect, useRef, useState } from 'react';
import { navigate } from '../../navigation';
import { currentWorkspace, workspaceUrl } from '../../workspace-context';
import { api } from '../../types';
import type { Profile } from '../../agents';
import { LegacyView } from '../legacy-view';
import { Button } from '../ui';
import { errorMessage } from '../requests';
import { CreateConversation } from './create-conversation';
import { ConversationPane } from './conversation';
import { chatRequest, conversationName, type Conversation, type Directory } from './model';
import './workspace-chat.css';

export function WorkspaceChat({ href }: { href: string }) {
  const url = new URL(href), agent = url.searchParams.get('agent'), selected = url.searchParams.get('conversation');
  const [directory, setDirectory] = useState<Directory>();
  const [chats, setChats] = useState<Conversation[]>([]);
  const [profiles, setProfiles] = useState<Profile[]>([]);
  const [error, setError] = useState('');
  const [query, setQuery] = useState('');
  const [chooser, setChooser] = useState<'channel' | 'direct'>();
  const [showList, setShowList] = useState(!agent && !selected);
  const refresh = useRef<() => Promise<void>>(async () => {});
  const owner = currentWorkspace()?.role === 'Owner';
  useEffect(() => {
    const abort = new AbortController(); let busy = false, again = false;
    const load = async () => {
      if (busy) { again = true; return; } busy = true;
      try {
        const [people, conversations] = await Promise.all([
          chatRequest<Directory>('/chat-directory', undefined, abort.signal),
          chatRequest<{ chats: Conversation[] }>('/chats', undefined, abort.signal),
        ]);
        if (!abort.signal.aborted) { setDirectory(people); setChats(conversations.chats); setError(''); }
      } catch (e) { if (!abort.signal.aborted) setError(errorMessage(e)); }
      finally { busy = false; if (again && !abort.signal.aborted) { again = false; void load(); } }
    };
    refresh.current = load; void load();
    const timer = window.setInterval(() => { if (!document.hidden) void load(); }, 5000);
    if (owner) void api<{ agents: { profile: Profile }[] }>('/agents', { signal: abort.signal }, 'local-agent')
      .then(data => { if (!abort.signal.aborted) setProfiles(data.agents.map(entry => entry.profile)); }).catch(() => {});
    return () => { abort.abort(); window.clearInterval(timer); };
  }, [owner]);
  useEffect(() => { setShowList(!agent && !selected); setChooser(undefined); }, [agent, selected]);
  const open = (id: string) => { setChooser(undefined); setShowList(false); navigate(workspaceUrl(`/?view=chat&conversation=${encodeURIComponent(id)}`)); void refresh.current(); };
  const openAgent = (id: string) => { setChooser(undefined); setShowList(false); navigate(workspaceUrl(`/?view=chat&agent=${encodeURIComponent(id)}`)); };
  const current = chats.find(chat => chat.id === selected);
  const matches = (name: string) => name.toLowerCase().includes(query.toLowerCase());
  const contact = (chat: Conversation) => {
    const name = directory ? conversationName(chat, directory) : chat.name;
    if (!matches(name)) return null;
    return <a key={chat.id} className="chat-contact" href={workspaceUrl(`/?view=chat&conversation=${chat.id}`)} aria-current={selected === chat.id && !agent ? 'page' : undefined}>
      <span className={`chat-avatar ${chat.kind === 'direct' ? '' : 'channel'}`}>{chat.kind === 'direct' ? name.slice(0, 1).toUpperCase() : '#'}</span><span>{name}</span>{chat.kind === 'direct' && chat.agents.length > 0 && <small>Agent</small>}
    </a>;
  };
  return <div className={`workspace-chat${showList ? ' show-chat-list' : ''}`} onClick={event => {
    if ((event.target as Element).closest('a.chat-contact')) setShowList(false);
    if ((event.target as Element).closest('#agent-sidebar-toggle, #agent-list-toggle')) setShowList(value => !value);
  }}>
    <aside className="chat-directory" aria-label="Conversations"><header><h1>Chat</h1>
      <Button className="ui-icon" aria-label="New message" title="New message" disabled={!directory} onClick={() => setChooser('direct')}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <path d="M12 4H5a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2h13a2 2 0 0 0 2-2v-7" />
          <path d="m16 3 5 5M10 14l-1 4 4-1L22 8a2 2 0 0 0-5-5Z" />
        </svg>
      </Button>
    </header>
      <input type="search" aria-label="Search conversations" placeholder="Find a conversation" value={query} onChange={e => setQuery(e.target.value)} />
      <div className="chat-directory-scroll"><section><header><h2>Channels</h2><Button className="ui-icon" aria-label="Create channel" title="Create channel" disabled={!directory} onClick={() => setChooser('channel')}>＋</Button></header>
        {chats.filter(chat => chat.kind !== 'direct').map(contact)}
        {!chats.some(chat => chat.kind !== 'direct') && <p className="chat-help">Organize work with your team.</p>}
      </section><section><header><h2>Direct messages</h2></header>{chats.filter(chat => chat.kind === 'direct').map(contact)}
        {owner && directory?.agents.filter(item => !item.shared && matches(item.name)).map(item => <a key={item.id} className="chat-contact" href={workspaceUrl(`/?view=chat&agent=${encodeURIComponent(item.id)}`)} aria-current={agent === item.id ? 'page' : undefined}><span className="chat-avatar agent">✧</span><span>{item.name === 'Ayati' ? 'Relay' : item.name}</span><small>Agent</small></a>)}
      </section></div>
      {!directory && !error && <p className="chat-help" role="status">Loading conversations…</p>}
      {error && <p className="chat-error" role="alert">{error} <Button onClick={() => void refresh.current()}>Retry</Button></p>}
    </aside>
    <div className="chat-content">
      {agent ? owner ? <LegacyView key={agent} page="chat" href={href} profile={profiles.find(profile => profile.id === agent)} /> : <p role="alert">Private agent chats require workspace owner access.</p>
        : current && directory ? <ConversationPane key={current.id} chat={current} directory={directory} back={() => setShowList(true)} />
        : <div className="chat-welcome"><Button className="chat-back" onClick={() => setShowList(true)}>‹ Chats</Button><div><span className="chat-welcome-mark">↗</span><h2>{selected ? 'Opening conversation' : 'A place to work together'}</h2><p>{selected ? error || (directory ? 'This conversation is unavailable or you no longer have access.' : 'Loading your conversations…') : 'Message a person, talk to an agent, or bring your team together in a channel.'}</p>{!selected && <Button primary disabled={!directory} onClick={() => setChooser('direct')}>New message</Button>}</div></div>}
    </div>
    {chooser && directory && <CreateConversation mode={chooser} directory={directory} close={() => setChooser(undefined)} created={open} privateAgent={openAgent} />}
  </div>;
}
