import { useRef, useState } from 'react';
import { Button, Dialog } from '../ui';
import { useMutation } from '../requests';
import { takeOver, useThreads } from './assistant-threads-model';
import type { Message } from './model';

/** Hand a direct chat over from one message (schema 136): you say what your
 * assistant should do, and it writes the first reply itself, as you. */
export function TakeOver({ message, name, assistantName, close }: { message: Message; name: string; assistantName: string; close: () => void }) {
  const { reload } = useThreads();
  const start = useMutation();
  const [instruction, setInstruction] = useState('');
  const field = useRef<HTMLTextAreaElement>(null);
  // One ID per exact instruction, so a retried start after a lost answer starts once.
  const attempt = useRef<{ key: string; id: string }>(undefined);
  const asked = instruction.trim();
  const submit = () => void start.run(async () => {
    if (!asked || !message.chat) return;
    if (attempt.current?.key !== asked) attempt.current = { key: asked, id: crypto.randomUUID() };
    await takeOver(attempt.current.id, message.chat, message.id, asked);
    reload(); close();
  });
  return <Dialog title={`Let ${assistantName} take over from here`} busy={start.busy} status={start.status} close={close} initialFocus={field}
    footer={<><Button onClick={close} disabled={start.busy}>Cancel</Button><Button primary disabled={!asked || start.busy} onClick={submit}>{start.busy ? 'Starting…' : 'Start'}</Button></>}>
    <div className="chat-threads">
      <blockquote className="chat-takeover-from"><strong>{name}</strong>{message.content.length > 280 ? `${message.content.slice(0, 280)}…` : message.content}</blockquote>
      <label>What should {assistantName} do?
        <textarea ref={field} value={instruction} maxLength={1000} disabled={start.busy} onChange={event => setInstruction(event.target.value)}
          onKeyDown={event => { if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) { event.preventDefault(); submit(); } }}
          placeholder="For example: tell him we are about to close the funding. Share dates only when confirmed." /></label>
      <p className="chat-help">{assistantName} replies to {name} as you, starting with this message, until you stop it. Only you and workspace admins can tell which messages it wrote.</p>
    </div>
  </Dialog>;
}
