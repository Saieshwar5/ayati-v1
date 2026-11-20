import { subscribeChanges } from './live';
import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { navigate } from '../../navigation';
import { workspaceUrl } from '../../workspace-context';
import { Button } from '../ui';
import { errorMessage } from '../requests';
import { CreateConversation } from './create-conversation';
import { ConversationPane } from './conversation';
import { chatRequest, conversationName, directPartner, type Conversation, type Directory } from './model';
import { ConversationFace, ParticipantName } from './conversation-face';
import { PeopleDirectory } from './people-directory';
import { ShowProfile, type ProfileTarget } from './profile-card';
import { ChatPanel, type PanelView } from './conversation-details';
import { AlertsPrompt, BellIcon } from './notify-controls';
import type { Level } from './notifications';
import './workspace-chat.css';
import './chat-layout.css';
import { PinButton, usePinActions } from './pin-control';
import { samePin, type Pin, type Preferences } from './organization-model';
import './organization.css';
import './people.css';
import { SearchConversations } from './search-conversations';
import { useDirectConversation } from './direct-conversation';
import { useUnreadCounts } from './unread-counts';
import { useConversationOrder } from './conversation-order';
import { applyPresence, clearPresence } from './presence';
import { PresenceWords, StatusEmoji } from './presence-view';
import { AssistantThreads, useAssistantThreads } from './assistant-threads-model';
import { ThreadMark } from './assistant-threads';
import { ForYouPage } from './for-you';
import { forYouCount } from './for-you-model';
import { chatPlaceUrl, forgetChatPlace, rememberChatPlace, rememberedChatPlace } from './last-chat-place';
import { nobodyForChannel, nobodyToMessage, onlyAssistantChat, startFacts, usePendingInvites } from './start-model';
import { ChatStart, inviteHref, StartTiles } from './chat-start';
import { AddSomeone } from './add-someone';
import { AgentFace, PersonFace } from '../faces';

export function WorkspaceChat({ href }: { href: string }) {
  const url = new URL(href), selected = url.searchParams.get('conversation');
  // Chat → For you takes the conversation's place; a conversation opened from it links back.
  const forYou = !selected && url.searchParams.get('section') === 'for-you';
  const forYouHref = workspaceUrl('/?view=chat&section=for-you');
  const [preferences, setPreferences] = useState<Preferences>();
  const [archivedVisible, showArchived] = useState(false);
  const sourceEpoch = useRef(0);
  const { counts: unread, mentions: mentioned } = useUnreadCounts();
  const [levels, setLevels] = useState<Record<string, Level>>({});
  const [directory, setDirectory] = useState<Directory>();
  const [chats, setChats] = useState<Conversation[]>([]);
  const [error, setError] = useState('');
  const [search, setSearch] = useState(false);
  const [chooser, setChooser] = useState<'channel' | 'direct'>();
  // Under a ＋ when nobody could be added yet: invite people or create an agent first.
  const [adding, setAdding] = useState<'channel' | 'direct'>();
  const plusButtons = { channel: useRef<HTMLButtonElement>(null), direct: useRef<HTMLButtonElement>(null) };
  const [people, showPeople] = useState(false);
  const [panel, setPanel] = useState<PanelView>();
  const [showList, setShowList] = useState(!selected && !forYou);
  const refresh = useRef<() => Promise<void>>(async () => {});
  const pins = usePinActions(preferences, next => setPreferences(previous => previous && previous.revision > next.revision ? previous : next), sourceEpoch, () => refresh.current());
  const pinned = (pin: Pin) => preferences?.pins.some(item => samePin(item, pin)) ?? false;
  const order = useConversationOrder(chats, refresh);
  // Arriving at a bare Chat link (the rail, or Back) reopens where you last were here.
  // Runs once per arrival: Chat unmounts on other pages. Replace keeps Back unchanged.
  const restored = useRef<string | undefined>(undefined);
  useLayoutEffect(() => {
    const arrived = new URL(window.location.href).searchParams;
    if (arrived.get('view') !== 'chat' || arrived.has('conversation') || arrived.has('section')) return;
    const place = rememberedChatPlace();
    if (!place) return;
    if (place.kind === 'conversation') restored.current = place.id;
    navigate(workspaceUrl(chatPlaceUrl(place)), { replace: true });
  }, []);
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
        if (!abort.signal.aborted) { applyPresence(people.presence); setDirectory(people); setChats(conversations.chats); setLevels(notify.levels ?? {}); setPreferences(previous => conversations.preferences && previous && previous.revision > conversations.preferences.revision ? previous : conversations.preferences); setError(''); }
      } catch (e) { if (!abort.signal.aborted) { setError(errorMessage(e)); if (e instanceof Error && 'status' in e && [400,401,403,404].includes(Number(e.status))) { setChats([]); setDirectory(undefined); setPreferences(undefined); clearPresence(); } } }
      finally { busy = false; if (again && !abort.signal.aborted) { again = false; void load(); } }
    };
    refresh.current = load; void load();
    // Only the bells: a level changed here or on another device (schema 140).
    const readLevels = () => void chatRequest<{ levels: Record<string, Level> }>('/chat-notifications', undefined, abort.signal)
      .then(notify => { if (!abort.signal.aborted) setLevels(notify.levels ?? {}); }).catch(() => {});
    const unsubscribe = subscribeChanges(change => {
      if (change.kind === 'access' || change.kind === 'reset') { sourceEpoch.current++; void load(); }
      // An agent or all agents were paused or resumed: their dots and message boxes change.
      if (change.kind === 'agents') void load();
      if (change.kind === 'levels') readLevels();
    });
    return () => { sourceEpoch.current++; abort.abort(); unsubscribe(); };
  }, []);
  useEffect(() => { setShowList(!selected && !forYou); setChooser(undefined); setSearch(false); }, [selected, forYou]);
  const open = (id: string) => { setChooser(undefined); setSearch(false); setShowList(false); navigate(workspaceUrl(`/?view=chat&conversation=${encodeURIComponent(id)}`)); void refresh.current(); };
  const openMessage = (id: string, message: string) => { setSearch(false); setShowList(false); navigate(workspaceUrl(`/?view=chat&conversation=${encodeURIComponent(id)}&message=${encodeURIComponent(message)}`)); };
  const direct = useDirectConversation(chats, open);
  const current = chats.find(chat => chat.id === selected);
  // Remember only what actually opened, never an unavailable link.
  useEffect(() => {
    if (current) rememberChatPlace({ kind: 'conversation', id: current.id });
    else if (forYou) rememberChatPlace({ kind: 'for-you' });
  }, [current?.id, forYou]);
  // A remembered conversation that is gone (deleted, access removed) falls back to the welcome.
  useEffect(() => {
    if (!directory || !selected || selected !== restored.current || current) return;
    restored.current = undefined; forgetChatPlace();
    navigate(workspaceUrl('/?view=chat'), { replace: true });
  }, [directory, selected, current]);
  // Your own assistant's chat is the first row, above everything else (schema 127).
  const mine = directory?.agents.find(agent => agent.assistant && agent.yours)?.id;
  const assistantChat = mine ? chats.find(chat => chat.kind === 'direct' && chat.agents.length === 1 && chat.agents[0] === mine && chat.users.length === 1) : undefined;
  const threads = useAssistantThreads(assistantChat ? mine : undefined, assistantChat?.id);
  const assistantName = directory?.agents.find(agent => agent.id === mine)?.name ?? 'Your assistant';
  const facts = directory ? startFacts(directory) : undefined;
  const invites = usePendingInvites(facts?.admin ?? false);
  const firstVisit = Boolean(facts) && onlyAssistantChat(chats, assistantChat?.id);
  /** A section's ＋: the ordinary dialog, or the choice to add someone first. */
  const plus = (mode: 'channel' | 'direct') => {
    if (!facts) return;
    if (mode === 'channel' ? nobodyForChannel(facts) : nobodyToMessage(facts)) setAdding(previous => previous === mode ? undefined : mode);
    else { setAdding(undefined); setChooser(mode); }
  };
  const closeAdding = (restoreFocus: boolean) => {
    const mode = adding; setAdding(undefined);
    if (restoreFocus && mode) plusButtons[mode].current?.focus();
  };
  const addSomeone = (mode: 'channel' | 'direct') => adding === mode && facts && <AddSomeone mode={mode} facts={facts} invites={invites} close={closeAdding} />;
  const plusButton = (mode: 'channel' | 'direct', label: string) => <Button ref={plusButtons[mode]} className="ui-icon" data-add-someone={mode}
    aria-label={label} title={label} aria-expanded={adding === mode || undefined} disabled={!directory} onClick={() => plus(mode)}>＋</Button>;
  const contact = (chat: Conversation) => {
    const name = directory ? conversationName(chat, directory) : chat.name;
    const partner = directory ? directPartner(chat, directory) : undefined;
    const count = unread[chat.id] ?? 0;
    return <div className="chat-contact-row" key={chat.id}><a className={`chat-contact${count > 0 ? ' chat-contact-unread' : ''}`}
      href={workspaceUrl(`/?view=chat&conversation=${chat.id}`)}
      aria-current={selected === chat.id ? 'page' : undefined}>
      {directory ? <ConversationFace chat={chat} directory={directory} /> : <span className="chat-avatar channel">#</span>}
      <span>{partner ? <ParticipantName kind={partner.kind} name={name} /> : <span className="chat-name-text">{name}</span>}
        {partner?.kind === 'user' && <><StatusEmoji id={partner.who.id} /><PresenceWords id={partner.who.id} /></>}{chat.archived && <small>Archived</small>}</span>
      {levels[chat.id] === 'none' && <span className="chat-muted" title="Muted"><BellIcon muted /></span>}
      <ThreadMark chat={chat.id} assistantName={assistantName} />
      {count > 0 && <small className={`chat-unread${mentioned[chat.id] ? ' chat-unread-mention' : ''}`} aria-label={`${count} unread messages${mentioned[chat.id] ? `, ${mentioned[chat.id]} for you` : ''}`}>{count}</small>}
    </a>{chat.id !== assistantChat?.id && <PinButton pin={{ kind: 'chat', id: chat.id }} name={name} preferences={preferences} busy={pins.busy} toggle={pins.toggle} />}</div>;
  };
  const activePinned = preferences?.pins.flatMap(pin => {
    const chat = chats.find(chat => chat.id === pin.id && !chat.archived && chat.id !== assistantChat?.id);
    return chat ? [contact(chat)] : [];
  }) ?? [];
  const archived = order.chats.filter(chat => chat.archived && chat.kind !== 'direct');
  const waitingForYou = forYouCount(chats, mentioned);
  const messageTarget = (target: ProfileTarget & { name: string }) => {
    const who = target.kind === 'user' ? directory?.users.find(user => user.id === target.id) : directory?.agents.find(agent => agent.id === target.id);
    if (!who) return;
    setPanel(undefined);
    void direct.choose({ ...who, kind: target.kind });
  };
  // The panel sits beside the conversation, so the People & agents dialog closes first.
  const showProfile = (target: ProfileTarget) => { showPeople(false); setPanel({ kind: 'profile', target }); };
  const details = {
    open: panel?.kind === 'details' && Boolean(current),
    toggle: () => setPanel(previous => previous?.kind === 'details' ? undefined : { kind: 'details' }),
    show: () => setPanel(previous => previous?.kind === 'details' ? previous : { kind: 'details' }),
    review: () => { if (current) setPanel({ kind: 'details', review: current.id }); },
  };
  return <AssistantThreads.Provider value={threads}><ShowProfile.Provider value={showProfile}><div className={`workspace-chat${showList ? ' show-chat-list' : ''}`} onClick={event => {
    if ((event.target as Element).closest('a.chat-contact, a.chat-for-you')) setShowList(false);
  }}>
    <aside className="chat-directory" aria-label="Conversations" {...order.listeners}><header><h1>Chat</h1>
      <Button className="ui-icon chat-search-button" aria-label="Search" title="Search conversations, people and messages" disabled={!directory} onClick={() => setSearch(true)}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true"><circle cx="10.5" cy="10.5" r="6.5" /><path d="m16 16 5 5" /></svg>
      </Button>
    </header>
      <div className="chat-directory-scroll"><AlertsPrompt />
        <section aria-label="For you"><div className="chat-contact-row"><a className={`chat-for-you${waitingForYou ? ' chat-for-you-new' : ''}`} href={forYouHref} aria-current={forYou ? 'page' : undefined}>
          <span className="chat-avatar chat-for-you-mark" aria-hidden="true"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8"><circle cx="12" cy="12" r="4" /><path d="M16 8v5a3 3 0 0 0 6 0v-1a10 10 0 1 0-4 8" /></svg></span>
          <span><span className="chat-name-text">For you</span></span>
          {waitingForYou > 0 && <small className="chat-unread chat-unread-mention" aria-label={`${waitingForYou} new for you`}>{waitingForYou > 99 ? '99+' : waitingForYou}</small>}
        </a></div></section>
        {assistantChat && <section aria-label="Your assistant">{contact(assistantChat)}</section>}{activePinned.length > 0 && <section aria-label="Pinned conversations"><header><h2>Pinned</h2><small>Personal</small></header>{activePinned}</section>}<section><header><h2>Channels</h2>{plusButton('channel', 'Create channel')}</header>
        {addSomeone('channel')}
        {order.chats.filter(chat => chat.kind !== 'direct' && !chat.archived && !pinned({ kind: 'chat', id: chat.id })).map(contact)}
      </section><section><header><h2>Direct messages</h2>{plusButton('direct', 'New message')}</header>{addSomeone('direct')}{order.chats.filter(chat => chat.kind === 'direct' && chat.id !== assistantChat?.id && !pinned({ kind: 'chat', id: chat.id })).map(contact)}
        {facts?.admin && !facts.people.length && adding !== 'direct' && <a className={`chat-invite-row${firstVisit ? ' chat-start-desktop-only' : ''}`} href={inviteHref()}><span aria-hidden="true">+</span>Invite people</a>}
      </section>{archived.length > 0 && <section><header><Button className="chat-archive-toggle" aria-expanded={archivedVisible} onClick={() => showArchived(value => !value)}>Archived channels ({archived.length})</Button></header>{archivedVisible && archived.map(contact)}</section>}
        {firstVisit && facts && <div className="chat-start-list"><StartTiles compact facts={facts} invites={invites} message={who => void direct.choose(who)} showAll={() => showPeople(true)} /></div>}</div>
      <Button className="chat-people-button" disabled={!directory} onClick={() => showPeople(true)}
        aria-label={directory ? `People & agents, ${directory.users.length} ${directory.users.length === 1 ? 'person' : 'people'}, ${directory.agents.length} ${directory.agents.length === 1 ? 'agent' : 'agents'}` : undefined}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true"><circle cx="9" cy="8" r="3.5" /><path d="M3 20a6 6 0 0 1 12 0" /><rect x="15" y="9" width="7" height="6" rx="2" /></svg>
        People &amp; agents{directory && <span className="chat-thread-faces chat-people-faces" aria-hidden="true">
          {directory.agents.slice(0, 1).map(agent => <AgentFace key={agent.id} agent={agent} className="chat-thread-face" />)}
          {directory.users.slice(0, 3).map(user => <PersonFace key={user.id} person={user} className="chat-thread-face" />)}
        </span>}
      </Button>
      {preferences?.available === false && <p className="chat-help">Personal pins and channel archive are unavailable in this workspace.</p>}
      {direct.status && <p className="chat-error" role="alert">{direct.status}</p>}
      {pins.status && <p className="chat-error" role="alert">{pins.status} <Button onClick={() => void refresh.current()}>Refresh conversations</Button></p>}
      {!directory && !error && <p className="chat-help" role="status">Loading conversations…</p>}
      {error && <p className="chat-error" role="alert">{error} <Button onClick={() => void refresh.current()}>Retry</Button></p>}
    </aside>
    <div className="chat-content">
      {forYou ? <ForYouPage directory={directory} back={() => setShowList(true)} />
        : current && directory ? <ConversationPane key={current.id} chat={current} directory={directory} returnTo={url.searchParams.get('from') === 'for-you' ? forYouHref : undefined} level={levels[current.id]} levelChanged={level => setLevels(previous => ({ ...previous, [current.id]: level }))} focus={url.searchParams.get('message') ?? undefined} back={() => setShowList(true)} details={details} />
        : <div className="chat-welcome"><Button className="chat-back" onClick={() => setShowList(true)}>‹ Chats</Button>{!selected && facts
          ? <ChatStart facts={facts} invites={invites} openAssistant={assistantChat ? () => open(assistantChat.id) : undefined}
            message={who => void direct.choose(who)} showAll={() => showPeople(true)} />
          : <div><h2>{selected ? 'Opening conversation' : 'Chat'}</h2><p role="status">{selected ? error || (directory ? 'This conversation is unavailable or you no longer have access.' : 'Loading your conversations…') : error || 'Loading your conversations…'}</p></div>}</div>}
    </div>
    {panel && directory && <ChatPanel view={panel} setView={setPanel} chat={current} directory={directory} chats={chats} message={messageTarget}
      level={current && levels[current.id]} levelChanged={level => { if (current) setLevels(previous => ({ ...previous, [current.id]: level })); }}
      pins={{ preferences, busy: pins.busy, toggle: pins.toggle }} changed={() => refresh.current()}
      left={() => { forgetChatPlace(); setPanel(undefined); setShowList(true); navigate(workspaceUrl('/?view=chat')); void refresh.current(); }} />}
    {search && directory && <SearchConversations directory={directory} chats={chats} close={() => setSearch(false)} opened={open} openMessage={openMessage} />}
    {chooser && directory && <CreateConversation mode={chooser} directory={directory} chats={chats} close={() => setChooser(undefined)} created={open} />}
    {people && directory && <PeopleDirectory directory={directory} chats={chats} preferences={preferences} togglePin={pins.toggle} close={() => showPeople(false)} opened={id => { showPeople(false); open(id); }} />}
  </div></ShowProfile.Provider></AssistantThreads.Provider>;
}
