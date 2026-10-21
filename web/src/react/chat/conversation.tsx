import { Attachments } from './attachments';
import { ChannelMembers, ChannelSettings } from './members';
import type { Attachment, Message } from './model';
import { Recipient } from './recipient';
import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { storageScope } from '../../workspace-context';
import { ChatMessage } from './message';
import { ChatThread } from './thread';
import { Button, Dialog } from '../ui';
import { errorMessage } from '../requests';
import { chatRequest, conversationName, type Conversation, type Directory, type History } from './model';

export function ConversationPane({ chat, directory, back }: { chat: Conversation; directory: Directory; back: () => void }) {
  const [uploadBusy, setUploadBusy] = useState(false);
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
  const [target, setTarget] = useState(directAgent);
  const draftKey = storageScope(`relay.chat.draft.${chat.id}`);
  const [draft, setDraft] = useState(() => { try { return sessionStorage.getItem(draftKey) ?? ''; } catch { return ''; } });
  const messages = useRef<HTMLDivElement>(null);
  const bottom = useRef(true);
  const input = useRef<HTMLTextAreaElement>(null);
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
        if (!abort.signal.aborted) { setHistory(data); setError(''); }
      } catch (e) { if (!abort.signal.aborted) { setError(errorMessage(e)); setHistory(undefined); } }
      finally { loading = false; if (queued && !abort.signal.aborted) { queued = false; void load(); } }
    };
    refresh.current = load; void load();
    const timer = window.setInterval(() => { if (!document.hidden) void load(); }, 2500);
    return () => { alive.current = false; abort.abort(); window.clearInterval(timer); };
  }, [chat.id]);
  useLayoutEffect(() => { if (bottom.current && messages.current) messages.current.scrollTop = messages.current.scrollHeight; }, [history]);
  useLayoutEffect(() => { const node = input.current; if (node) { node.style.height = 'auto'; node.style.height = `${Math.min(node.scrollHeight, 140)}px`; } }, [draft]);
  const updateDraft = (text: string) => { setDraft(text); try { sessionStorage.setItem(draftKey, text); } catch { /* Optional local draft. */ } };
  const send = async () => {
    if (pending.current || uploadBusy || (!draft.trim() && !files.length) || !history) return;
    pending.current = true; setBusy(true); setError('');
    if (!attempt.current || attempt.current.content !== (draft || "Shared files") || attempt.current.agent !== (target || undefined) || JSON.stringify(attempt.current.attachments) !== JSON.stringify(files.map(f => f.id))) attempt.current = { id: crypto.randomUUID(), content: draft || "Shared files", agent: target || undefined, attachments: files.map(f => f.id) };
    const submission = attempt.current;
    try {
      await chatRequest(`/chats/${chat.id}/messages`, submission);
      if (!alive.current) return;
      updateDraft(''); setFiles([]); attempt.current = undefined; if (submission.agent && chat.kind !== 'direct') setThread(submission.id); bottom.current = true; await refresh.current(); input.current?.focus();
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
  const title = conversationName(chat, directory);
  const availableAgents = directory.agents.filter(agent => history?.agents.includes(agent.id));
  return <section className="chat-conversation" aria-label={`Conversation with ${title}`}>
    <header className="chat-pane-header"><Button className="chat-back" onClick={back}>‹ Chats</Button><span className="chat-avatar">{chat.kind === 'direct' ? title.slice(0, 1).toUpperCase() : '#'}</span><div><h2 title={title}>{chat.kind !== 'direct' && '# '}{title}</h2></div><Button onClick={() => setDetails(true)}>Details</Button></header>
    <div className="chat-message-list" role="log" aria-label="Messages" ref={messages} onScroll={() => { const node = messages.current!; bottom.current = node.scrollHeight - node.scrollTop - node.clientHeight < 80; }}>
      {(cursor === undefined ? history?.next_before : cursor) && <Button disabled={loadingOlder} onClick={() => void loadOlder()}>Load older messages</Button>}
      {!history && !error && <p className="chat-empty" role="status">Loading messages…</p>}
      {history?.messages.length === 0 && <div className="chat-empty"><h3>{chat.kind === 'direct' ? `Say hello to ${title}` : `Welcome to #${title}`}</h3><p>{chat.kind === 'direct' ? 'Start the conversation below.' : 'Keep the conversation and work together here.'}</p></div>}
      {visibleHistory?.messages.filter(message => !message.thread).map(message => <ChatMessage key={message.id} message={message} directory={directory} replies={allMessages.filter(reply => reply.thread === message.id).length} openThread={() => setThread(message.id)} />)}
    </div>
    {error && <p className="chat-error" role="alert">{error} <Button onClick={() => void refresh.current()}>Retry</Button></p>}
    <Attachments pending={setUploadBusy} chat={chat.id} value={files} change={setFiles} disabled={busy} />
    <form className="chat-compose" onSubmit={e => { e.preventDefault(); void send(); }}>
      <textarea ref={input} aria-label="Message" placeholder="Message…" maxLength={12000} rows={1} value={draft} disabled={busy} onChange={e => updateDraft(e.target.value)} />
      <div>{!directAgent && availableAgents.length > 0 ? <Recipient agents={availableAgents} value={target} change={setTarget} disabled={busy} /> : null}<Button primary type="submit" aria-label="Send message" disabled={uploadBusy || busy || !history || (!draft.trim() && !files.length)}>↑</Button></div>
    </form>
    {visibleHistory && thread && allMessages.some(message => message.id === thread) && <ChatThread key={thread} chat={chat.id} root={allMessages.find(message => message.id === thread)!} history={visibleHistory} directory={directory} close={() => setThread(undefined)} refresh={() => refresh.current()} />}
    {details && <Dialog title="Conversation details" drawer close={() => setDetails(false)}><h3>{title}</h3><p>{chat.description || "Messages follow this channel’s access settings."}</p><p>Shared agents use this conversation only. Public channels are visible to all workspace members; private channels require membership. Internal work operations require a separate grant.</p><h3>Members</h3>{chat.kind !== 'direct' && <><ChannelSettings chat={chat} directory={directory} /><ChannelMembers chat={chat} directory={directory} /></>}{chat.users.map(id => <p key={id}>{directory.users.find(user => user.id === id)?.name ?? 'Former member'}</p>)}{chat.agents.map(id => <p key={id}>{directory.agents.find(agent => agent.id === id)?.name ?? 'Agent'} · Agent</p>)}</Dialog>}
  </section>;
}
