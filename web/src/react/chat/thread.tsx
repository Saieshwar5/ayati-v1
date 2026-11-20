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
import './thread-panel.css';

export function ChatThread({ chat, root, history, directory, close, people = [], focus, archived, direct, place, read }: { archived?: boolean; focus?: string; people?: Mention[]; chat: string; root: Message; history: History; directory: Directory; close: () => void; refresh: () => Promise<void>;
  /** Hears each reply read here, so the conversation's thread line stops calling it new. */
  read?: (id: string) => void;
  /** In a direct chat with an agent, every reply asks that agent. */
  direct?: string;
  /** Where these messages are, as a forward of one says. */
  place?: string }) {
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
  // Only the messages scroll. Stay at the newest reply unless the reader moved
  // up to read, or the thread opened on one particular message.
  const scroller = useRef<HTMLDivElement>(null);
  const atEnd = useRef(true);
  const noteScroll = () => { const node = scroller.current; if (node) atEnd.current = node.scrollHeight - node.scrollTop - node.clientHeight < 40; };
  useEffect(() => {
    const node = scroller.current;
    if (node && !focus && atEnd.current) node.scrollTop = node.scrollHeight;
  }, [replies.length, focus, root.id]);
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
      let agents = direct ? requestAgents(text, direct) : requestAgents(text);
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
  const continuingName = continuing && directory.agents.find(agent => agent.id === continuing)?.name;
  const count = replies.length;
  return <Dialog title={archived ? 'Archived thread' : 'Thread'} drawer busy={mutation.busy} status={mutation.status} close={close}>
    <div className="chat-thread-panel">
      <div ref={scroller} className="chat-thread-scroll" onScroll={noteScroll}>
        <div className="chat-thread-root"><ChatMessage message={root} directory={directory} place={place} /></div>
        <p className="chat-thread-divider">{count ? `${count} ${count === 1 ? 'reply' : 'replies'}` : 'No replies yet'}</p>
        <div ref={transcript} role="log" aria-label="Thread replies">{replies.map(message => <ChatMessage focused={message.id === focus} key={message.id} message={message} directory={directory} place={place} read={read} />)}<PendingMessages chat={chat} thread={root.id} agentName={id => directory.agents.find(agent => agent.id === id)?.name} /></div>
        {!count && !archived && <p className="chat-thread-empty"><strong>Start the thread</strong>Replies stay here, under this message, so the channel stays tidy.</p>}
      </div>
      <div className="chat-thread-reply">
        <div className="chat-thread-reply-line">
          {!archived && <p className="chat-thread-route" role="note">{waiting
            ? <>Your reply answers <strong>{directory.agents.find(agent => agent.id === waiting)?.name ?? 'the agent'}</strong>’s question</>
            : continuingName
            ? <>Replies go to <strong>{continuingName}</strong> · @mention someone to change</>
            : 'Replies go to the thread. @mention an agent to ask it.'}</p>}
          <ThreadFollow chat={chat} thread={root.id} />
        </div>
        {archived && <p className="chat-archive-banner">This archived channel is closed to new thread replies. Your draft is kept.</p>}
        <ChatComposer chat={chat} files={files} setFiles={setFiles} value={text} change={update} send={send} disabled={mutation.busy || Boolean(archived)}
          sendDisabled={!text.trim() && !files.length} label="Reply in thread" placeholder="Reply…"
          candidates={[...people, ...agents.map(agent => ({ ...agent, kind: 'agent' as const }))]} />
      </div>
    </div>
  </Dialog>;
}
