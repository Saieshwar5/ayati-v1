import { Attachments } from './attachments';
import type { Attachment } from './model';
import { Recipient } from './recipient';
import { useRef, useState } from 'react';
import { storageScope } from '../../workspace-context';
import { Button, Dialog } from '../ui';
import { useMutation } from '../requests';
import { chatRequest, type Directory, type History, type Message } from './model';
import { ChatMessage } from './message';

export function ChatThread({ chat, root, history, directory, close, refresh }: { chat: string; root: Message; history: History; directory: Directory; close: () => void; refresh: () => Promise<void> }) {
  const key = storageScope(`relay.chat.thread.${chat}.${root.id}`);
  const [text, setText] = useState(() => { try { return sessionStorage.getItem(key) ?? ''; } catch { return ''; } });
  const [uploadBusy, setUploadBusy] = useState(false);
  const [files, setFiles] = useState<Attachment[]>([]);
  const [target, setTarget] = useState('');
  const mutation = useMutation();
  const attempt = useRef<{ id: string; content: string; thread: string; agent?: string; attachments: string[] }>(undefined);
  const update = (value: string) => { setText(value); try { sessionStorage.setItem(key, value); } catch { /* Optional draft. */ } };
  const send = () => mutation.run(async () => {
    if (uploadBusy || (!text.trim() && !files.length)) return;
    if (!attempt.current || attempt.current.content !== (text || "Shared files") || attempt.current.agent !== (target || undefined) || JSON.stringify(attempt.current.attachments) !== JSON.stringify(files.map(f => f.id))) attempt.current = { id: crypto.randomUUID(), content: text || "Shared files", thread: root.id, agent: target || undefined, attachments: files.map(f => f.id) };
    await chatRequest(`/chats/${chat}/messages`, attempt.current);
    update(''); setFiles([]); attempt.current = undefined; await refresh();
  });
  const agents = directory.agents.filter(agent => history.agents.includes(agent.id));
  return <Dialog title="Thread" drawer busy={mutation.busy} status={mutation.status} close={close}>
    <div className="chat-thread-root"><ChatMessage message={root} directory={directory} /></div>
    <div role="log" aria-label="Thread replies">{history.messages.filter(message => message.thread === root.id).map(message => <ChatMessage key={message.id} message={message} directory={directory} />)}</div>
    <Attachments pending={setUploadBusy} chat={chat} value={files} change={setFiles} disabled={mutation.busy} />
    <form className="chat-compose chat-thread-compose" onSubmit={e => { e.preventDefault(); void send(); }}>
      <textarea aria-label="Reply in thread" placeholder="Reply in this thread…" rows={1} maxLength={12000} value={text} onChange={e => update(e.target.value)} />
      <div>{agents.length > 0 ? <Recipient agents={agents} value={target} change={setTarget} disabled={mutation.busy} /> : null}<Button primary type="submit" disabled={uploadBusy || (!text.trim() && !files.length) || mutation.busy} aria-label="Send reply">↑</Button></div>
    </form>
  </Dialog>;
}
