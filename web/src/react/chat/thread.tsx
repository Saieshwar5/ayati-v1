import { PendingMessages } from './pending-messages';
import { enqueue } from './outbox';
import { ChatComposer } from './attachments';
import { requestAgents, type Mention } from './mentions';
import type { Attachment } from './model';
import { useEffect, useRef, useState } from 'react';
import { storageScope, workspaceSession } from '../../workspace-context';
import { Dialog } from '../ui';
import { useMutation } from '../requests';
import { filesOnlyContent, type Directory, type History, type Message } from './model';
import { ChatMessage } from './message';
import { askedBy, continuingAgent, namesSomeone } from './thread-line';
import { ThreadFollow } from './thread-follow';

export function ChatThread({ chat, root, history, directory, close, people = [], focus, archived, direct, invitable = [], place }: { archived?: boolean; focus?: string; people?: Mention[]; chat: string; root: Message; history: History; directory: Directory; close: () => void; refresh: () => Promise<void>;
  /** In a direct chat with an agent, every reply asks that agent. */
  direct?: string;
  /** In your chat with your own agent: agents you may mention to work with it here (schema 125). */
  invitable?: Mention[]; place?: string }) {
  const transcript = useRef<HTMLDivElement>(null);
  const replies = history.messages.filter(message => message.thread === root.id);
  const me = workspaceSession?.user?.id ?? '';
  // An agent asked this person here: their reply answers it, not a new request.
  const waiting = askedBy(replies, me);
  const continuing = waiting ? undefined : direct ?? continuingAgent(root, replies, history.agents);
  useEffect(() => {
    if (!focus) return;
    const target = [...(transcript.current?.querySelectorAll<HTMLElement>('[data-message-id]') ?? [])].find(node => node.dataset.messageId === focus);
    target?.scrollIntoView?.({ block: 'center' });
  }, [focus, root.id]);
  const key = storageScope(`relay.chat.thread.${chat}.${root.id}`);
  const [text, setText] = useState(() => { try { return sessionStorage.getItem(key) ?? ''; } catch { return ''; } });
  const [files, setFiles] = useState<Attachment[]>([]);
  const mutation = useMutation();
  const attempt = useRef<{ id: string; content: string; thread: string; agents?: string[]; attachments: string[] }>(undefined);
  const update = (value: string) => { setText(value); try { sessionStorage.setItem(key, value); } catch { /* Optional draft. */ } };
  /** Resolves true once the reply is in the outbox; a voice recording joins the draft's files. */
  const send = async (voice?: Attachment) => {
    let sent = false;
    await mutation.run(async () => {
      const attachments = (voice ? [...files, voice] : files).map(f => f.id);
      if (archived || !text.trim() && !attachments.length) return;
      let agents = direct ? requestAgents(text, direct, invitable.length > 0) : requestAgents(text);
      // A plain reply goes to the agent a person last asked here; naming anyone
      // sends it only to whom it names.
      if (!agents.length && continuing && !namesSomeone(text)) agents = [continuing];
      const asked = agents.length ? agents : undefined;
      const content = text || filesOnlyContent;
      if (!attempt.current || attempt.current.content !== content || JSON.stringify(attempt.current.agents) !== JSON.stringify(asked) || JSON.stringify(attempt.current.attachments) !== JSON.stringify(attachments)) attempt.current = { id: crypto.randomUUID(), content, thread: root.id, agents: asked, attachments };
      await enqueue(chat, attempt.current);
      update(''); setFiles([]); attempt.current = undefined; sent = true;
    });
    return sent;
  };
  const agents = directory.agents.filter(agent => history.agents.includes(agent.id));
  const guests = [...new Set(replies.filter(message => message.kind === 'agent' && !history.agents.includes(message.author)).map(message => message.author))].map(id => directory.agents.find(agent => agent.id === id)?.name ?? 'Agent');
  const continuingName = continuing && directory.agents.find(agent => agent.id === continuing)?.name;
  return <Dialog title={archived ? 'Archived thread' : 'Thread'} drawer busy={mutation.busy} status={mutation.status} close={close}>
    <ThreadFollow chat={chat} thread={root.id} />
    {guests.length > 0 && <p className="chat-visibility-note">{guests.join(' and ')} joined this thread only. {guests.length === 1 ? 'It' : 'They'} can’t see the rest of this chat. {guests.length === 1 ? 'Its' : 'Their'} owners and admins can see the work done here.</p>}
    <div className="chat-thread-root"><ChatMessage message={root} directory={directory} place={place} /></div>
    <div ref={transcript} role="log" aria-label="Thread replies">{history.messages.filter(message => message.thread === root.id).map(message => <ChatMessage focused={message.id === focus} key={message.id} message={message} directory={directory} place={place} />)}<PendingMessages chat={chat} thread={root.id} agentName={id => directory.agents.find(agent => agent.id === id)?.name} /></div>
    {!archived && <p className="chat-thread-route" role="note">{waiting
      ? <>Your reply answers <strong>{directory.agents.find(agent => agent.id === waiting)?.name ?? 'the agent'}</strong>’s question</>
      : continuingName
      ? <>Replies go to <strong>{continuingName}</strong> · @mention someone to change</>
      : 'Replies go to the thread. @mention an agent to ask it.'}</p>}
    {archived && <p className="chat-archive-banner">This archived channel is closed to new thread replies. Your draft is kept.</p>}
    <ChatComposer chat={chat} files={files} setFiles={setFiles} value={text} change={update} send={send} disabled={mutation.busy || Boolean(archived)}
      sendDisabled={!text.trim() && !files.length} label="Reply in thread" placeholder="Reply…"
      candidates={[...people, ...agents.map(agent => ({ ...agent, kind: 'agent' as const })), ...invitable]} />
  </Dialog>;
}
