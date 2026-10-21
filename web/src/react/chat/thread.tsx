import { PendingMessages } from './pending-messages';
import { enqueue } from './outbox';
import { ChatComposer } from './attachments';
import { requestAgent, type Mention } from './mentions';
import type { Attachment } from './model';
import { useRef, useState } from 'react';
import { storageScope } from '../../workspace-context';
import { Dialog } from '../ui';
import { useMutation } from '../requests';
import { type Directory, type History, type Message } from './model';
import { ChatMessage } from './message';

export function ChatThread({ chat, root, history, directory, close, people = [] }: { people?: Mention[]; chat: string; root: Message; history: History; directory: Directory; close: () => void; refresh: () => Promise<void> }) {
  const key = storageScope(`relay.chat.thread.${chat}.${root.id}`);
  const [text, setText] = useState(() => { try { return sessionStorage.getItem(key) ?? ''; } catch { return ''; } });
  const [files, setFiles] = useState<Attachment[]>([]);
  const mutation = useMutation();
  const attempt = useRef<{ id: string; content: string; thread: string; agent?: string; attachments: string[] }>(undefined);
  const update = (value: string) => { setText(value); try { sessionStorage.setItem(key, value); } catch { /* Optional draft. */ } };
  const send = () => mutation.run(async () => {
    if (!text.trim() && !files.length) return;
    const target = requestAgent(text);
    if (!attempt.current || attempt.current.content !== (text || "Shared files") || attempt.current.agent !== (target || undefined) || JSON.stringify(attempt.current.attachments) !== JSON.stringify(files.map(f => f.id))) attempt.current = { id: crypto.randomUUID(), content: text || "Shared files", thread: root.id, agent: target || undefined, attachments: files.map(f => f.id) };
    await enqueue(chat, attempt.current);
    update(''); setFiles([]); attempt.current = undefined;
  });
  const agents = directory.agents.filter(agent => history.agents.includes(agent.id));
  return <Dialog title="Thread" drawer busy={mutation.busy} status={mutation.status} close={close}>
    <div className="chat-thread-root"><ChatMessage message={root} directory={directory} /></div>
    <div role="log" aria-label="Thread replies">{history.messages.filter(message => message.thread === root.id).map(message => <ChatMessage key={message.id} message={message} directory={directory} />)}<PendingMessages chat={chat} thread={root.id} /></div>
    <ChatComposer chat={chat} files={files} setFiles={setFiles} value={text} change={update} send={() => void send()} disabled={mutation.busy}
      sendDisabled={!text.trim() && !files.length} label="Reply in thread" placeholder="Reply…"
      candidates={[...people, ...agents.map(agent => ({ ...agent, kind: 'agent' as const }))]} />
  </Dialog>;
}
