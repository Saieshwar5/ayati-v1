import { displayAgentName } from '../../agents';
import { subscribeChanges } from './live';
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
import { chatRequest, conversationKind, conversationName, type Conversation, type Directory } from './model';
import './workspace-chat.css';
import './chat-layout.css';
import { ApprovalInbox } from '../approvals/approval-inbox';
import { PinButton, usePinActions } from './pin-control';
import { samePin, type Pin, type Preferences } from './organization-model';
import './organization.css';
import { SearchConversations } from './search-conversations';

export function WorkspaceChat({ href }: { href: string }) {
  const url = new URL(href), agent = url.searchParams.get('agent'), selected = url.searchParams.get('conversation');
  const [preferences, setPreferences] = useState<Preferences>();
  const [archivedVisible, showArchived] = useState(false);
  const sourceEpoch = useRef(0);
  const [unread, setUnread] = useState<Record<string, number>>({});
  const [directory, setDirectory] = useState<Directory>();
  const [chats, setChats] = useState<Conversation[]>([]);
  const [profiles, setProfiles] = useState<Profile[]>([]);
  const [error, setError] = useState('');
  const [search, setSearch] = useState(false);
  const [approvals, showApprovals] = useState(false);
  const [chooser, setChooser] = useState<'channel' | 'direct'>();
  const [showList, setShowList] = useState(!agent && !selected);
  const refresh = useRef<() => Promise<void>>(async () => {});
  const owner = currentWorkspace()?.role === 'Owner';
  const privateAccess = owner && directory?.role === 'Owner';
  const pins = usePinActions(preferences, next => setPreferences(previous => previous && previous.revision > next.revision ? previous : next), sourceEpoch, () => refresh.current());
  const pinned = (pin: Pin) => preferences?.pins.some(item => samePin(item, pin)) ?? false;
  useEffect(() => {
    const abort = new AbortController(); let busy = false, again = false;
    const load = async () => {
      if (busy) { again = true; return; } busy = true;
      try {
        const [people, conversations] = await Promise.all([
          chatRequest<Directory>('/chat-directory', undefined, abort.signal),
          chatRequest<{ chats: Conversation[]; preferences?: Preferences }>('/chats', undefined, abort.signal),
        ]);
        if (!abort.signal.aborted) { setDirectory(people); setChats(conversations.chats); setPreferences(previous => conversations.preferences && previous && previous.revision > conversations.preferences.revision ? previous : conversations.preferences); setError(''); }
      } catch (e) { if (!abort.signal.aborted) { setError(errorMessage(e)); if (e instanceof Error && 'status' in e && [400,401,403,404].includes(Number(e.status))) { setChats([]); setDirectory(undefined); setUnread({}); setPreferences(undefined); setProfiles([]); } } }
      finally { busy = false; if (again && !abort.signal.aborted) { again = false; void load(); } }
    };
    refresh.current = load; void load();
    let unreadTimer: number | undefined;
    const counts = async () => { try { const data = await chatRequest<{ unread: { chat: string; count: number }[] }>('/unread', undefined, abort.signal); if (!abort.signal.aborted) setUnread(Object.fromEntries(data.unread.map(item => [item.chat, item.count]))); } catch { /* Keep last known counts during an outage. */ } };
    void counts();
    const unsubscribe = subscribeChanges(change => {
      if (change.kind === 'access' || change.kind === 'reset') { sourceEpoch.current++; void load(); }
      if (unreadTimer === undefined) unreadTimer = window.setTimeout(() => { unreadTimer = undefined; void counts(); }, 500);
    });
    if (owner) void api<{ agents: { profile: Profile }[] }>('/agents', { signal: abort.signal }, 'local-agent')
      .then(data => { if (!abort.signal.aborted) setProfiles(data.agents.map(entry => entry.profile)); }).catch(() => {});
    return () => { sourceEpoch.current++; abort.abort(); unsubscribe(); window.clearTimeout(unreadTimer); };
  }, [owner]);
  useEffect(() => { setShowList(!agent && !selected); setChooser(undefined); setSearch(false); }, [agent, selected]);
  const open = (id: string) => { setChooser(undefined); setSearch(false); setShowList(false); navigate(workspaceUrl(`/?view=chat&conversation=${encodeURIComponent(id)}`)); void refresh.current(); };
  const openAgent = (id: string) => { setChooser(undefined); setSearch(false); setShowList(false); navigate(workspaceUrl(`/?view=chat&agent=${encodeURIComponent(id)}`)); };
  const current = chats.find(chat => chat.id === selected);
  const contact = (chat: Conversation) => {
    const name = directory ? conversationName(chat, directory) : chat.name;
    const kind = directory ? conversationKind(chat, directory) : chat.kind === 'direct' ? 'Person' : 'Channel';
    return <div className="chat-contact-row" key={chat.id}><a className="chat-contact"
      href={workspaceUrl(`/?view=chat&conversation=${chat.id}`)}
      aria-current={selected === chat.id && !agent ? 'page' : undefined}>
      <span className={`chat-avatar ${kind.toLowerCase()}`}>
        {kind === 'Channel' ? '#' : kind === 'Agent' ? '✧' : name.slice(0, 1).toUpperCase()}
      </span>
      <span>{name}{(kind !== 'Channel' || chat.archived) && <small>{chat.archived ? 'Archived' : kind}</small>}</span>
      {unread[chat.id] > 0 && <small className="chat-unread" aria-label={`${unread[chat.id]} unread messages`}>{unread[chat.id]}</small>}
    </a><PinButton pin={{ kind: 'chat', id: chat.id }} name={name} preferences={preferences} busy={pins.busy} toggle={pins.toggle} /></div>;
  };
  const localContact = (item: Directory['agents'][number]) => <div className="chat-contact-row" key={`local-${item.id}`}><a className="chat-contact" href={workspaceUrl(`/?view=chat&agent=${encodeURIComponent(item.id)}`)} aria-current={agent === item.id ? 'page' : undefined}><span className="chat-avatar agent">✧</span><span>{displayAgentName(item)}<small>Agent</small></span></a><PinButton pin={{ kind: 'local_agent', id: item.id }} name={displayAgentName(item)} preferences={preferences} busy={pins.busy} toggle={pins.toggle} /></div>;
  const activePinned = preferences?.pins.flatMap(pin => {
    if (pin.kind === 'chat') { const chat = chats.find(chat => chat.id === pin.id && !chat.archived); return chat ? [contact(chat)] : []; }
    const local = privateAccess ? directory?.agents.find(item => !item.shared && item.id === pin.id) : undefined;
    return local ? [localContact(local)] : [];
  }) ?? [];
  const archived = chats.filter(chat => chat.archived && chat.kind !== 'direct');
  return <div className={`workspace-chat${showList ? ' show-chat-list' : ''}`} onClick={event => {
    if ((event.target as Element).closest('a.chat-contact')) setShowList(false);
    if ((event.target as Element).closest('#agent-sidebar-toggle, #agent-list-toggle')) setShowList(value => !value);
  }}>
    <aside className="chat-directory" aria-label="Conversations"><header><h1>Chat</h1><Button className="ui-icon" aria-label="My approvals" title="My approvals" disabled={!directory} onClick={() => showApprovals(true)}>✓</Button>
      <Button className="ui-icon chat-search-button" aria-label="Search conversations" title="Search channels, people and agents" disabled={!directory} onClick={() => setSearch(true)}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true"><circle cx="10.5" cy="10.5" r="6.5" /><path d="m16 16 5 5" /></svg>
      </Button>
    </header>
      <div className="chat-directory-scroll">{activePinned.length > 0 && <section aria-label="Pinned conversations"><header><h2>Pinned</h2><small>Personal</small></header>{activePinned}</section>}<section><header><h2>Channels</h2><Button className="ui-icon" aria-label="Create channel" title="Create channel" disabled={!directory} onClick={() => setChooser('channel')}>＋</Button></header>
        {chats.filter(chat => chat.kind !== 'direct' && !chat.archived && !pinned({ kind: 'chat', id: chat.id })).map(contact)}
        {!chats.some(chat => chat.kind !== 'direct' && !chat.archived) && <p className="chat-help">Organize work with your team.</p>}
      </section><section><header><h2>Direct messages</h2><Button className="ui-icon" aria-label="New message" title="New message" disabled={!directory} onClick={() => setChooser('direct')}>＋</Button></header>{chats.filter(chat => chat.kind === 'direct' && !pinned({ kind: 'chat', id: chat.id })).map(contact)}
        {privateAccess && directory?.agents.filter(item => !item.shared && !pinned({ kind: 'local_agent', id: item.id })).map(localContact)}
      </section>{archived.length > 0 && <section><header><Button className="chat-archive-toggle" aria-expanded={archivedVisible} onClick={() => showArchived(value => !value)}>Archived channels ({archived.length})</Button></header>{archivedVisible && archived.map(contact)}</section>}</div>
      {preferences?.available === false && <p className="chat-help">Personal pins and channel archive are unavailable in this workspace.</p>}
      {pins.status && <p className="chat-error" role="alert">{pins.status} <Button onClick={() => void refresh.current()}>Refresh conversations</Button></p>}
      {!directory && !error && <p className="chat-help" role="status">Loading conversations…</p>}
      {error && <p className="chat-error" role="alert">{error} <Button onClick={() => void refresh.current()}>Retry</Button></p>}
    </aside>
    <div className="chat-content">
      {agent ? !directory ? <p role={error ? 'alert' : 'status'}>{error || 'Loading private conversation…'}</p> : privateAccess ? <LegacyView key={agent} page="chat" href={href} profile={profiles.find(profile => profile.id === agent)} /> : <p role="alert">Private agent chats require workspace owner access.</p>
        : current && directory ? <ConversationPane key={current.id} chat={current} directory={directory} changed={() => refresh.current()} focus={url.searchParams.get('message') ?? undefined} back={() => setShowList(true)} />
        : <div className="chat-welcome"><Button className="chat-back" onClick={() => setShowList(true)}>‹ Chats</Button><div><span className="chat-welcome-mark">↗</span><h2>{selected ? 'Opening conversation' : 'A place to work together'}</h2><p>{selected ? error || (directory ? 'This conversation is unavailable or you no longer have access.' : 'Loading your conversations…') : 'Message a person, talk to an agent, or bring your team together in a channel.'}</p>{!selected && <Button primary disabled={!directory} onClick={() => setChooser('direct')}>New message</Button>}</div></div>}
    </div>
    {approvals && <ApprovalInbox close={() => showApprovals(false)} />}
    {search && directory && <SearchConversations directory={directory} chats={chats} close={() => setSearch(false)} opened={open} privateAgent={openAgent} />}
    {chooser && directory && <CreateConversation mode={chooser} directory={directory} chats={chats} close={() => setChooser(undefined)} created={open} privateAgent={openAgent} />}
  </div>;
}
