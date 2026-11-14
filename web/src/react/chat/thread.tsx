import { PendingMessages } from './pending-messages';
import { enqueue, type Submission } from './outbox';
import { ChatComposer } from './attachments';
import { requestAgents, type Mention } from './mentions';
import type { Attachment } from './model';
import { useEffect, useRef, useState } from 'react';
import { storageScope } from '../../workspace-context';
import { useMutation } from '../requests';
import { filesOnlyContent, type Directory, type History, type Message } from './model';
import { ChatMessage } from './message';
import { SidePanel } from './side-panel';

/** A thread beside the conversation (schema 115), in the panel Details and
 * profiles use, so the conversation stays readable while replying. */
export function ChatThread({ chat, place, root, history, directory, close, people = [], focus, archived }: { archived?: boolean; focus?: string; people?: Mention[]; chat: string;
  /** Where "Also send to …" posts: `#orders`, or "the conversation" for a direct message. */ place: string;
  root: Message; history: History; directory: Directory; close: () => void; refresh: () => Promise<void> }) {
  const transcript = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!focus) return;
    const target = [...(transcript.current?.querySelectorAll<HTMLElement>('[data-message-id]') ?? [])].find(node => node.dataset.messageId === focus);
    target?.scrollIntoView?.({ block: 'center' });
  }, [focus, root.id]);
  const key = storageScope(`relay.chat.thread.${chat}.${root.id}`);
  const [text, setText] = useState(() => { try { return sessionStorage.getItem(key) ?? ''; } catch { return ''; } });
  const [files, setFiles] = useState<Attachment[]>([]);
  const [broadcast, setBroadcast] = useState(false);
  const mutation = useMutation();
  const attempt = useRef<Submission & { thread: string }>(undefined);
  const update = (value: string) => { setText(value); try { sessionStorage.setItem(key, value); } catch { /* Optional draft. */ } };
  /** Resolves true once the reply is in the outbox; a voice recording joins the draft's files. */
  const send = async (voice?: Attachment) => {
    let sent = false;
    await mutation.run(async () => {
      const attachments = (voice ? [...files, voice] : files).map(f => f.id);
      if (archived || !text.trim() && !attachments.length) return;
      let agents = requestAgents(text);
      // In an agent's thread a reply asks that agent, unless the text names others.
      if (!agents.length && root.kind === 'agent' && history.agents.includes(root.author)) agents = [root.author];
      const asked = agents.length ? agents : undefined;
      const content = text || filesOnlyContent;
      const shared = broadcast ? true as const : undefined;
      // The same payload keeps its ID, so a retried send can't post twice.
      if (!attempt.current || attempt.current.content !== content || JSON.stringify(attempt.current.agents) !== JSON.stringify(asked) || JSON.stringify(attempt.current.attachments) !== JSON.stringify(attachments) || attempt.current.broadcast !== shared)
        attempt.current = { id: crypto.randomUUID(), content, thread: root.id, agents: asked, attachments, ...(shared ? { broadcast: shared } : {}) };
      await enqueue(chat, attempt.current);
      update(''); setFiles([]); setBroadcast(false); attempt.current = undefined; sent = true;
    });
    return sent;
  };
  const agents = directory.agents.filter(agent => history.agents.includes(agent.id));
  const replies = history.messages.filter(message => message.thread === root.id);
  return <SidePanel className="chat-thread-panel" label={archived ? 'Archived thread' : 'Thread'} close={close}
    title={<>{archived ? 'Archived thread' : 'Thread'}<small className="chat-thread-place">{place}</small></>}>
    <div ref={transcript} className="chat-thread-replies" role="log" aria-label="Thread replies" aria-busy={mutation.busy}>
      <div className="chat-thread-root"><ChatMessage message={root} directory={directory} /></div>
      {replies.map(message => <ChatMessage focused={message.id === focus} key={message.id} message={message} directory={directory} />)}
      <PendingMessages chat={chat} thread={root.id} agentName={id => directory.agents.find(agent => agent.id === id)?.name} />
    </div>
    {mutation.status && <p className="chat-error" role="alert">{mutation.status}</p>}
    {archived && <p className="chat-archive-banner">This archived channel is closed to new thread replies. Your draft is kept.</p>}
    <ChatComposer chat={chat} files={files} setFiles={setFiles} value={text} change={update} send={send} disabled={mutation.busy || Boolean(archived)}
      sendDisabled={!text.trim() && !files.length} label="Reply in thread" placeholder="Reply…"
      candidates={[...people, ...agents.map(agent => ({ ...agent, kind: 'agent' as const }))]} />
    {!archived && <label className="chat-thread-broadcast">
      <input type="checkbox" checked={broadcast} disabled={mutation.busy} onChange={event => setBroadcast(event.target.checked)} />
      Also send to {place}
    </label>}
  </SidePanel>;
}
