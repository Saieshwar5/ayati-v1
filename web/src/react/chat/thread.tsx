import { useRef, useState } from 'react';
import { storageScope } from '../../workspace-context';
import { Button, Dialog } from '../ui';
import { useMutation } from '../requests';
import { chatRequest, type Directory, type History, type Message } from './model';
import { ChatMessage } from './message';

export function ChatThread({ chat, root, history, directory, close, refresh }: { chat: string; root: Message; history: History; directory: Directory; close: () => void; refresh: () => Promise<void> }) {
  const key = storageScope(`relay.chat.thread.${chat}.${root.id}`);
  const [text, setText] = useState(() => { try { return sessionStorage.getItem(key) ?? ''; } catch { return ''; } });
  const [target, setTarget] = useState('');
  const mutation = useMutation();
  const attempt = useRef<{ id: string; content: string; thread: string; agent?: string }>(undefined);
  const update = (value: string) => { setText(value); try { sessionStorage.setItem(key, value); } catch { /* Optional draft. */ } };
  const send = () => mutation.run(async () => {
    if (!text.trim()) return;
    if (!attempt.current || attempt.current.content !== text || attempt.current.agent !== (target || undefined)) attempt.current = { id: crypto.randomUUID(), content: text, thread: root.id, agent: target || undefined };
    await chatRequest(`/chats/${chat}/messages`, attempt.current);
    update(''); attempt.current = undefined; await refresh();
  });
  const agents = directory.agents.filter(agent => history.agents.includes(agent.id));
  return <Dialog title="Thread" drawer busy={mutation.busy} status={mutation.status} close={close}>
    <div className="chat-thread-root"><ChatMessage message={root} directory={directory} /></div>
    <div role="log" aria-label="Thread replies">{history.messages.filter(message => message.thread === root.id).map(message => <ChatMessage key={message.id} message={message} directory={directory} />)}</div>
    <form className="chat-compose chat-thread-compose" onSubmit={e => { e.preventDefault(); void send(); }}>
      <textarea aria-label="Reply in thread" placeholder="Reply in this thread…" rows={2} maxLength={12000} value={text} onChange={e => update(e.target.value)} />
      <div>{agents.length > 0 ? <label className="chat-target">To<select aria-label="Thread reply recipient" value={target} onChange={e => setTarget(e.target.value)}><option value="">Everyone</option>{agents.map(agent => <option key={agent.id} value={agent.id}>Ask {agent.name}</option>)}</select></label> : <span className="chat-help">Reply to this conversation</span>}<Button primary type="submit" disabled={!text.trim() || mutation.busy}>Send reply</Button></div>
    </form>
  </Dialog>;
}
