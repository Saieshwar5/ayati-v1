import { PendingMessages } from './pending-messages';
import { enqueue } from './outbox';
import { ChatComposer } from './attachments';
import { requestAgent, type Mention } from './mentions';
import type { Attachment } from './model';
import { useEffect, useRef, useState } from 'react';
import { storageScope } from '../../workspace-context';
import { Dialog } from '../ui';
import { useMutation } from '../requests';
import { filesOnlyContent, type Directory, type History, type Message } from './model';
import { ChatMessage } from './message';

export function ChatThread({ chat, root, history, directory, close, people = [], focus, archived }: { archived?: boolean; focus?: string; people?: Mention[]; chat: string; root: Message; history: History; directory: Directory; close: () => void; refresh: () => Promise<void> }) {
  const transcript = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!focus) return;
    const target = [...(transcript.current?.querySelectorAll<HTMLElement>('[data-message-id]') ?? [])].find(node => node.dataset.messageId === focus);
    target?.scrollIntoView?.({ block: 'center' });
  }, [focus, root.id]);
  const key = storageScope(`relay.chat.thread.${chat}.${root.id}`);
  const [text, setText] = useState(() => { try { return sessionStorage.getItem(key) ?? ''; } catch { return ''; } });
  const [files, setFiles] = useState<Attachment[]>([]);
  const mutation = useMutation();
  const attempt = useRef<{ id: string; content: string; thread: string; agent?: string; attachments: string[] }>(undefined);
  const update = (value: string) => { setText(value); try { sessionStorage.setItem(key, value); } catch { /* Optional draft. */ } };
  /** Resolves true once the reply is in the outbox; a voice recording joins the draft's files. */
  const send = async (voice?: Attachment) => {
    let sent = false;
    await mutation.run(async () => {
      const attachments = (voice ? [...files, voice] : files).map(f => f.id);
      if (archived || !text.trim() && !attachments.length) return;
      let target = requestAgent(text);
      // In an agent's thread a reply asks that agent, unless the text names another.
      if (!target && root.kind === 'agent' && history.agents.includes(root.author)) target = root.author;
      const content = text || filesOnlyContent;
      if (!attempt.current || attempt.current.content !== content || attempt.current.agent !== (target || undefined) || JSON.stringify(attempt.current.attachments) !== JSON.stringify(attachments)) attempt.current = { id: crypto.randomUUID(), content, thread: root.id, agent: target || undefined, attachments };
      await enqueue(chat, attempt.current);
      update(''); setFiles([]); attempt.current = undefined; sent = true;
    });
    return sent;
  };
  const agents = directory.agents.filter(agent => history.agents.includes(agent.id));
  return <Dialog title={archived ? 'Archived thread' : 'Thread'} drawer busy={mutation.busy} status={mutation.status} close={close}>
    <div className="chat-thread-root"><ChatMessage message={root} directory={directory} /></div>
    <div ref={transcript} role="log" aria-label="Thread replies">{history.messages.filter(message => message.thread === root.id).map(message => <ChatMessage focused={message.id === focus} key={message.id} message={message} directory={directory} />)}<PendingMessages chat={chat} thread={root.id} /></div>
    {archived && <p className="chat-archive-banner">This archived channel is closed to new thread replies. Your draft is kept.</p>}
    <ChatComposer chat={chat} files={files} setFiles={setFiles} value={text} change={update} send={send} disabled={mutation.busy || Boolean(archived)}
      sendDisabled={!text.trim() && !files.length} label="Reply in thread" placeholder="Reply…"
      candidates={[...people, ...agents.map(agent => ({ ...agent, kind: 'agent' as const }))]} />
  </Dialog>;
}
