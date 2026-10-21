import { PendingMessages } from './pending-messages';
import { enqueue, confirmSent, outboxSnapshot, subscribeOutbox } from './outbox';
import { acknowledge, liveStatus, subscribeStatus, subscribeChanges } from './live';
import { ChatComposer } from './attachments';
import { requestAgent } from './mentions';
import { ChannelMembers, ChannelSettings } from './members';
import type { Attachment, Message } from './model';
import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore } from 'react';
import { storageScope } from '../../workspace-context';
import { ChatMessage } from './message';
import { ChatThread } from './thread';
import { Button, Dialog } from '../ui';
import { errorMessage } from '../requests';
import { chatRequest, conversationName, type Conversation, type Directory, type History } from './model';

export function ConversationPane({ chat, directory, back }: { chat: Conversation; directory: Directory; back: () => void }) {
  const pendingMessages = useSyncExternalStore(subscribeOutbox, outboxSnapshot);
  const connection = useSyncExternalStore(subscribeStatus, liveStatus);
  const incoming = useRef(new Map<string, Message>());
  const [files, setFiles] = useState<Attachment[]>([]);
  const [older, setOlder] = useState<Message[]>([]);
  const [cursor, setCursor] = useState<number | null>();
  const [loadingOlder, setLoadingOlder] = useState(false);
  const [history, setHistory] = useState<History>();
  const [error, setError] = useState('');
  const [details, setDetails] = useState(false);
  const [thread, setThread] = useState<string>();
  const [busy, setBusy] = useState(false);
  const directAgent = chat.kind === 'direct' ? chat.agents[0] ?? '' : '';
  const draftKey = storageScope(`relay.chat.draft.${chat.id}`);
  const [draft, setDraft] = useState(() => { try { return sessionStorage.getItem(draftKey) ?? ''; } catch { return ''; } });
  const messages = useRef<HTMLDivElement>(null);
  const bottom = useRef(true);
  const pending = useRef(false);
  const attempt = useRef<{ id: string; content: string; agent?: string; attachments: string[] }>(undefined);
  const refresh = useRef<() => Promise<void>>(async () => {});
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    const abort = new AbortController(); let loading = false, queued = false;
    const load = async () => {
      if (loading) { queued = true; return; } loading = true;
      try { const data = await chatRequest<History>(`/chats/${chat.id}/messages`, undefined, abort.signal);
        if (!abort.signal.aborted) {
          const merged = [...new Map([...data.messages, ...incoming.current.values()].map(m => [m.id, m])).values()].sort((a,b)=>(a.sequence ?? 0)-(b.sequence ?? 0));
          setHistory({ ...data, messages: merged }); acknowledge(chat.id, data.messages); void confirmSent(data.messages).catch(() => {}); setError('');
        }
      } catch (e) { if (!abort.signal.aborted) { setError(errorMessage(e)); if (e instanceof Error && 'status' in e && [400,401,403,404].includes(Number(e.status))) { setHistory(undefined); setOlder([]); incoming.current.clear(); } } }
      finally { loading = false; if (queued && !abort.signal.aborted) { queued = false; void load(); } }
    };
    refresh.current = load; void load();
    const unsubscribe = subscribeChanges(change => {
      if (change.kind === 'reset' || change.kind === 'access') { incoming.current.clear(); void load(); }
      if (change.chat === chat.id && change.message) {
        const message = change.message; void confirmSent([message]).catch(() => {}); incoming.current.set(message.id, message);
        if (incoming.current.size > 400) incoming.current.delete(incoming.current.keys().next().value!);
        setOlder(old => old.map(m => m.id === message.id ? message : m));
        setHistory(previous => previous ? { ...previous, messages: [...new Map([...previous.messages, message].map(m => [m.id,m])).values()].sort((a,b)=>(a.sequence ?? 0)-(b.sequence ?? 0)).slice(-400) } : previous);
      }
    });
    return () => { alive.current = false; abort.abort(); unsubscribe(); };
  }, [chat.id]);
  useLayoutEffect(() => { if (bottom.current && messages.current) messages.current.scrollTop = messages.current.scrollHeight; }, [history, pendingMessages]);
  const updateDraft = (text: string) => { setDraft(text); try { sessionStorage.setItem(draftKey, text); } catch { /* Optional local draft. */ } };
  const send = async () => {
    if (pending.current || (!draft.trim() && !files.length)) return;
    pending.current = true; setBusy(true); setError('');
    let target: string | undefined;
    try { target = requestAgent(draft, directAgent); } catch (e) { setError(errorMessage(e)); pending.current = false; setBusy(false); return; }
    if (!attempt.current || attempt.current.content !== (draft || "Shared files") || attempt.current.agent !== (target || undefined) || JSON.stringify(attempt.current.attachments) !== JSON.stringify(files.map(f => f.id))) attempt.current = { id: crypto.randomUUID(), content: draft || "Shared files", agent: target || undefined, attachments: files.map(f => f.id) };
    const submission = attempt.current;
    try {
      await enqueue(chat.id, submission);
      if (!alive.current) return;
      updateDraft(''); setFiles([]); attempt.current = undefined; bottom.current = true;
    } catch (e) { if (alive.current) setError(errorMessage(e)); }
    finally { pending.current = false; if (alive.current) setBusy(false); }
  };
  const loadOlder = async () => {
    const before = cursor === undefined ? history?.next_before : cursor;
    if (!before || loadingOlder) return; setLoadingOlder(true);
    try { const page = await chatRequest<History>(`/chats/${chat.id}/messages?before=${before}`); if (alive.current) { bottom.current = false; setOlder(previous => [...page.messages, ...previous]); setCursor(page.next_before ?? null); } }
    catch (e) { if (alive.current) setError(errorMessage(e)); }
    finally { if (alive.current) setLoadingOlder(false); }
  };
  const allMessages = [...new Map([...older, ...(history?.messages ?? [])].map(m => [m.id, m])).values()].sort((a, b) => (a.sequence ?? a.at) - (b.sequence ?? b.at));
  const visibleHistory = history ? { ...history, messages: allMessages } : undefined;
  const timeline = visibleHistory?.messages.filter(message => !message.thread) ?? [];
  const title = conversationName(chat, directory);
  const availableAgents = directory.agents.filter(agent => history?.agents.includes(agent.id));
  return <section className="chat-conversation" aria-label={`Conversation with ${title}`}>
    <header className="chat-pane-header"><Button className="chat-back" onClick={back}>‹ Chats</Button><span className="chat-avatar">{chat.kind === 'direct' ? title.slice(0, 1).toUpperCase() : '#'}</span><div><h2 title={title}>{chat.kind !== 'direct' && '# '}{title}</h2></div><Button onClick={() => setDetails(true)}>Details</Button></header>
    <p className="chat-connection" role="status">{connection}</p>
    <div className="chat-message-list" role="log" aria-label="Messages" ref={messages} onScroll={() => { const node = messages.current!; bottom.current = node.scrollHeight - node.scrollTop - node.clientHeight < 80; }}>
      {(cursor === undefined ? history?.next_before : cursor) && <Button disabled={loadingOlder} onClick={() => void loadOlder()}>Load older messages</Button>}
      {!history && !error && <p className="chat-empty" role="status">Loading messages…</p>}
      {history?.messages.length === 0 && <div className="chat-empty"><h3>{chat.kind === 'direct' ? `Say hello to ${title}` : `Welcome to #${title}`}</h3><p>{chat.kind === 'direct' ? 'Start the conversation below.' : 'Keep the conversation and work together here.'}</p></div>}
      {timeline.map((message, index) => <ChatMessage compact={index > 0 && timeline[index-1].author === message.author && timeline[index-1].kind === message.kind && message.at-timeline[index-1].at < 120} key={message.id} message={message} directory={directory} replies={allMessages.filter(reply => reply.thread === message.id).length} openThread={() => setThread(message.id)} />)}
      <PendingMessages chat={chat.id} />
    </div>
    {error && <p className="chat-error" role="alert">{error} <Button onClick={() => void refresh.current()}>Retry</Button></p>}
    <ChatComposer chat={chat.id} files={files} setFiles={setFiles} value={draft} change={updateDraft} send={() => void send()} disabled={busy} sendDisabled={!draft.trim() && !files.length}
      placeholder={chat.kind === 'direct' ? 'Message…' : 'Message… Type @ to mention someone'}
      candidates={[
        ...directory.users.filter(user => chat.users.includes(user.id) || chat.visibility === 'public').map(user => ({ ...user, kind: 'user' as const })),
        ...availableAgents.map(agent => ({ ...agent, kind: 'agent' as const })),
      ]} />
    {visibleHistory && thread && allMessages.some(message => message.id === thread) && <ChatThread people={directory.users.filter(user => chat.users.includes(user.id) || chat.visibility === 'public').map(user => ({ ...user, kind: 'user' as const }))} key={thread} chat={chat.id} root={allMessages.find(message => message.id === thread)!} history={visibleHistory} directory={directory} close={() => setThread(undefined)} refresh={() => refresh.current()} />}
    {details && <Dialog title="Conversation details" drawer close={() => setDetails(false)}><h3>{title}</h3><p>{chat.description || "Messages follow this channel’s access settings."}</p><p>Shared agents use this conversation only. Public channels are visible to all workspace members; private channels require membership. Internal work operations require a separate grant.</p><h3>Members</h3>{chat.kind !== 'direct' && <><ChannelSettings chat={chat} directory={directory} /><ChannelMembers chat={chat} directory={directory} /></>}{chat.users.map(id => <p key={id}>{directory.users.find(user => user.id === id)?.name ?? 'Former member'}</p>)}{chat.agents.map(id => <p key={id}>{directory.agents.find(agent => agent.id === id)?.name ?? 'Agent'} · Agent</p>)}</Dialog>}
  </section>;
}
