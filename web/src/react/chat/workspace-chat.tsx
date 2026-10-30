import { subscribeChanges } from './live';
import { useEffect, useRef, useState } from 'react';
import { navigate } from '../../navigation';
import { workspaceUrl } from '../../workspace-context';
import { Button } from '../ui';
import { errorMessage } from '../requests';
import { CreateConversation } from './create-conversation';
import { ConversationPane } from './conversation';
import { chatRequest, conversationName, directPartner, type Conversation, type Directory } from './model';
import { ConversationFace, ParticipantName } from './conversation-face';
import { PeopleDirectory } from './people-directory';
import { ProfileCard, ShowProfile, type ProfileTarget } from './profile-card';
import { AlertsPrompt, BellIcon } from './notify-controls';
import type { Level } from './notifications';
import './workspace-chat.css';
import './chat-layout.css';
import { ApprovalInbox } from '../approvals/approval-inbox';
import { PinButton, usePinActions } from './pin-control';
import { samePin, type Pin, type Preferences } from './organization-model';
import './organization.css';
import './people.css';
import { SearchConversations } from './search-conversations';
import { useDirectConversation } from './direct-conversation';

export function WorkspaceChat({ href }: { href: string }) {
  const url = new URL(href), selected = url.searchParams.get('conversation');
  const [preferences, setPreferences] = useState<Preferences>();
  const [archivedVisible, showArchived] = useState(false);
  const sourceEpoch = useRef(0);
  const [unread, setUnread] = useState<Record<string, number>>({});
  const [mentioned, setMentioned] = useState<Record<string, number>>({});
  const [levels, setLevels] = useState<Record<string, Level>>({});
  const [directory, setDirectory] = useState<Directory>();
  const [chats, setChats] = useState<Conversation[]>([]);
  const [error, setError] = useState('');
  const [search, setSearch] = useState(false);
  const [approvals, showApprovals] = useState(false);
  const [chooser, setChooser] = useState<'channel' | 'direct'>();
  const [people, showPeople] = useState(false);
  const [profile, setProfile] = useState<ProfileTarget>();
  const [showList, setShowList] = useState(!selected);
  const refresh = useRef<() => Promise<void>>(async () => {});
  const pins = usePinActions(preferences, next => setPreferences(previous => previous && previous.revision > next.revision ? previous : next), sourceEpoch, () => refresh.current());
  const pinned = (pin: Pin) => preferences?.pins.some(item => samePin(item, pin)) ?? false;
  useEffect(() => {
    const abort = new AbortController(); let busy = false, again = false;
    const load = async () => {
      if (busy) { again = true; return; } busy = true;
      try {
        const [people, conversations, notify] = await Promise.all([
          chatRequest<Directory>('/chat-directory', undefined, abort.signal),
          chatRequest<{ chats: Conversation[]; preferences?: Preferences }>('/chats', undefined, abort.signal),
          chatRequest<{ levels: Record<string, Level> }>('/chat-notifications', undefined, abort.signal).catch(() => ({ levels: {} })),
        ]);
        if (!abort.signal.aborted) { setDirectory(people); setChats(conversations.chats); setLevels(notify.levels ?? {}); setPreferences(previous => conversations.preferences && previous && previous.revision > conversations.preferences.revision ? previous : conversations.preferences); setError(''); }
      } catch (e) { if (!abort.signal.aborted) { setError(errorMessage(e)); if (e instanceof Error && 'status' in e && [400,401,403,404].includes(Number(e.status))) { setChats([]); setDirectory(undefined); setUnread({}); setPreferences(undefined); } } }
      finally { busy = false; if (again && !abort.signal.aborted) { again = false; void load(); } }
    };
    refresh.current = load; void load();
    let unreadTimer: number | undefined;
    const counts = async () => { try { const data = await chatRequest<{ unread: { chat: string; count: number; mentions?: number }[] }>('/unread', undefined, abort.signal); if (!abort.signal.aborted) { setUnread(Object.fromEntries(data.unread.map(item => [item.chat, item.count]))); setMentioned(Object.fromEntries(data.unread.map(item => [item.chat, item.mentions ?? 0]))); } } catch { /* Keep last known counts during an outage. */ } };
    void counts();
    const unsubscribe = subscribeChanges(change => {
      if (change.kind === 'access' || change.kind === 'reset') { sourceEpoch.current++; void load(); }
      if (unreadTimer === undefined) unreadTimer = window.setTimeout(() => { unreadTimer = undefined; void counts(); }, 500);
    });
    return () => { sourceEpoch.current++; abort.abort(); unsubscribe(); window.clearTimeout(unreadTimer); };
  }, []);
  useEffect(() => { setShowList(!selected); setChooser(undefined); setSearch(false); }, [selected]);
  const open = (id: string) => { setChooser(undefined); setSearch(false); setShowList(false); navigate(workspaceUrl(`/?view=chat&conversation=${encodeURIComponent(id)}`)); void refresh.current(); };
  const direct = useDirectConversation(chats, open);
  const current = chats.find(chat => chat.id === selected);
  const contact = (chat: Conversation) => {
    const name = directory ? conversationName(chat, directory) : chat.name;
    const partner = directory ? directPartner(chat, directory) : undefined;
    const count = unread[chat.id] ?? 0;
    return <div className="chat-contact-row" key={chat.id}><a className={`chat-contact${count > 0 ? ' chat-contact-unread' : ''}`}
      href={workspaceUrl(`/?view=chat&conversation=${chat.id}`)}
      aria-current={selected === chat.id ? 'page' : undefined}>
      {directory ? <ConversationFace chat={chat} directory={directory} /> : <span className="chat-avatar channel">#</span>}
      <span>{partner ? <ParticipantName kind={partner.kind} name={name} /> : <span className="chat-name-text">{name}</span>}{chat.archived && <small>Archived</small>}</span>
      {levels[chat.id] === 'none' && <span className="chat-muted" title="Muted"><BellIcon muted /></span>}
      {count > 0 && <small className={`chat-unread${mentioned[chat.id] ? ' chat-unread-mention' : ''}`} aria-label={`${count} unread messages${mentioned[chat.id] ? `, ${mentioned[chat.id]} for you` : ''}`}>{count}</small>}
    </a><PinButton pin={{ kind: 'chat', id: chat.id }} name={name} preferences={preferences} busy={pins.busy} toggle={pins.toggle} /></div>;
  };
  const activePinned = preferences?.pins.flatMap(pin => {
    const chat = chats.find(chat => chat.id === pin.id && !chat.archived);
    return chat ? [contact(chat)] : [];
  }) ?? [];
  const archived = chats.filter(chat => chat.archived && chat.kind !== 'direct');
  const messageTarget = (target: ProfileTarget & { name: string }) => {
    const who = target.kind === 'user' ? directory?.users.find(user => user.id === target.id) : directory?.agents.find(agent => agent.id === target.id);
    if (!who) return;
    setProfile(undefined);
    void direct.choose({ ...who, kind: target.kind });
  };
  return <ShowProfile.Provider value={setProfile}><div className={`workspace-chat${showList ? ' show-chat-list' : ''}`} onClick={event => {
    if ((event.target as Element).closest('a.chat-contact')) setShowList(false);
  }}>
    <aside className="chat-directory" aria-label="Conversations"><header><h1>Chat</h1><Button className="ui-icon" aria-label="My approvals" title="My approvals" disabled={!directory} onClick={() => showApprovals(true)}>✓</Button>
      <Button className="ui-icon chat-search-button" aria-label="Search conversations" title="Search channels, people and agents" disabled={!directory} onClick={() => setSearch(true)}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true"><circle cx="10.5" cy="10.5" r="6.5" /><path d="m16 16 5 5" /></svg>
      </Button>
    </header>
      <div className="chat-directory-scroll"><AlertsPrompt />{activePinned.length > 0 && <section aria-label="Pinned conversations"><header><h2>Pinned</h2><small>Personal</small></header>{activePinned}</section>}<section><header><h2>Channels</h2><Button className="ui-icon" aria-label="Create channel" title="Create channel" disabled={!directory} onClick={() => setChooser('channel')}>＋</Button></header>
        {chats.filter(chat => chat.kind !== 'direct' && !chat.archived && !pinned({ kind: 'chat', id: chat.id })).map(contact)}
        {!chats.some(chat => chat.kind !== 'direct' && !chat.archived) && <p className="chat-help">Organize work with your team.</p>}
      </section><section><header><h2>Direct messages</h2><Button className="ui-icon" aria-label="New message" title="New message" disabled={!directory} onClick={() => setChooser('direct')}>＋</Button></header>{chats.filter(chat => chat.kind === 'direct' && !pinned({ kind: 'chat', id: chat.id })).map(contact)}
      </section>{archived.length > 0 && <section><header><Button className="chat-archive-toggle" aria-expanded={archivedVisible} onClick={() => showArchived(value => !value)}>Archived channels ({archived.length})</Button></header>{archivedVisible && archived.map(contact)}</section>}</div>
      <Button className="chat-people-button" disabled={!directory} onClick={() => showPeople(true)}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true"><circle cx="9" cy="8" r="3.5" /><path d="M3 20a6 6 0 0 1 12 0" /><rect x="15" y="9" width="7" height="6" rx="2" /></svg>
        People &amp; agents{directory && <small>{directory.users.length} · {directory.agents.length}</small>}
      </Button>
      {preferences?.available === false && <p className="chat-help">Personal pins and channel archive are unavailable in this workspace.</p>}
      {direct.status && <p className="chat-error" role="alert">{direct.status}</p>}
      {pins.status && <p className="chat-error" role="alert">{pins.status} <Button onClick={() => void refresh.current()}>Refresh conversations</Button></p>}
      {!directory && !error && <p className="chat-help" role="status">Loading conversations…</p>}
      {error && <p className="chat-error" role="alert">{error} <Button onClick={() => void refresh.current()}>Retry</Button></p>}
    </aside>
    <div className="chat-content">
      {current && directory ? <ConversationPane key={current.id} chat={current} directory={directory} level={levels[current.id]} levelChanged={level => setLevels(previous => ({ ...previous, [current.id]: level }))} changed={() => refresh.current()} focus={url.searchParams.get('message') ?? undefined} back={() => setShowList(true)} />
        : <div className="chat-welcome"><Button className="chat-back" onClick={() => setShowList(true)}>‹ Chats</Button><div><span className="chat-welcome-mark">↗</span><h2>{selected ? 'Opening conversation' : 'A place to work together'}</h2><p>{selected ? error || (directory ? 'This conversation is unavailable or you no longer have access.' : 'Loading your conversations…') : 'Message a person, talk to an agent, or bring your team together in a channel.'}</p>{!selected && <Button primary disabled={!directory} onClick={() => setChooser('direct')}>New message</Button>}</div></div>}
    </div>
    {approvals && <ApprovalInbox close={() => showApprovals(false)} />}
    {search && directory && <SearchConversations directory={directory} chats={chats} close={() => setSearch(false)} opened={open} />}
    {chooser && directory && <CreateConversation mode={chooser} directory={directory} chats={chats} close={() => setChooser(undefined)} created={open} />}
    {people && directory && <PeopleDirectory directory={directory} chats={chats} preferences={preferences} togglePin={pins.toggle} close={() => showPeople(false)} opened={id => { showPeople(false); open(id); }} />}
    {profile && directory && <ProfileCard target={profile} directory={directory} chats={chats} close={() => setProfile(undefined)} message={messageTarget} />}
  </div></ShowProfile.Provider>;
}
