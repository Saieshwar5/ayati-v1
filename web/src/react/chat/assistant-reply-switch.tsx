import { useRef, useState } from 'react';
import { Button } from '../ui';
import { useMutation } from '../requests';
import { AssistantMark } from './assistant-threads';
import { positionOf, setReplies, threadIn, useThreads, type Position } from './assistant-threads-model';

const CHOICES: { position: Position; label: string; help: (name: string) => string }[] = [
  { position: 'off', label: 'Off', help: name => `${name} does nothing here unless you mention it.` },
  { position: 'ask', label: 'Ask me first', help: name => `${name} writes each reply into your message box. You send it.` },
  { position: 'auto', label: 'Reply for me', help: name => `${name} sends replies as you, and asks when it needs your decision.` },
];

/** In the header of a direct chat with one person or one agent: what your
 * assistant does there (schema 141). A quiet mark while it is off, a pill while
 * it is on, and one menu to change it. Replying for you starts only from the
 * explicit button, because those replies go out without you reading them. */
export function ReplySwitch({ chat, assistantName, partnerName }: { chat: string; assistantName: string; partnerName: string }) {
  const { threads, reload } = useThreads();
  const thread = threadIn(threads, chat);
  const position = positionOf(thread);
  const [open, setOpen] = useState(false);
  const [starting, setStarting] = useState(false);
  const [instruction, setInstruction] = useState('');
  const toggle = useRef<HTMLButtonElement>(null);
  const mutation = useMutation();
  // One ID per exact choice, so a retried request after a lost answer turns it on once.
  const attempt = useRef<{ key: string; id: string }>(undefined);
  // Closing puts the keyboard back on the mark, whose menu item is gone.
  const close = () => { setOpen(false); setStarting(false); toggle.current?.focus(); };
  const choose = (next: Position, asked?: string) => void mutation.run(async () => {
    const key = `${next}\n${asked ?? ''}`;
    if (attempt.current?.key !== key) attempt.current = { key, id: crypto.randomUUID() };
    await setReplies(attempt.current.id, chat, next, asked);
    attempt.current = undefined; reload(); close();
  });
  const pick = (next: Position) => {
    if (next === position) { setStarting(false); return; }
    if (next !== 'auto') { choose(next); return; }
    setInstruction(thread?.instruction ?? ''); setStarting(true);
  };
  const words = position === 'off' ? '' : thread?.working ? `${assistantName} is writing…` : position === 'ask' ? `${assistantName} asks you first` : `${assistantName} is replying`;
  const state = `${assistantName} in this chat: ${CHOICES.find(choice => choice.position === position)!.label}`;
  // Escape closes only this menu, not a panel or dialog around it.
  return <span className="chat-replies" onKeyDown={event => { if (open && event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); close(); } }}>
    <Button ref={toggle} className={position === 'off' ? 'ui-icon chat-replies-button' : `chat-thread-pill ${position}`} aria-label={state} aria-expanded={open} title={state}
      onClick={() => open ? close() : setOpen(true)}><AssistantMark size={position === 'off' ? 17 : 14} />{words && <span>{words}</span>}</Button>
    {open && <div className="chat-replies-menu" role="group" aria-label={`${assistantName} in your chat with ${partnerName}`}>
      <div role="radiogroup" aria-label="What it does here">
        {CHOICES.map(choice => <button type="button" role="radio" aria-checked={starting ? choice.position === 'auto' : position === choice.position} key={choice.position} disabled={mutation.busy} onClick={() => pick(choice.position)}>
          <span>{choice.label}<small>{choice.help(assistantName)}</small></span>
        </button>)}
      </div>
      {starting && <form className="chat-replies-start" onSubmit={event => { event.preventDefault(); choose('auto', instruction.trim()); }}>
        <label>What should {assistantName} do? <small>Optional</small>
          <textarea value={instruction} maxLength={1000} rows={2} autoFocus disabled={mutation.busy} onChange={event => setInstruction(event.target.value)}
            placeholder="For example: settle a time for Friday. Share dates only when confirmed." /></label>
        <small>Its replies go to {partnerName} as your messages, without you reading them first. It stops by itself when the matter is settled.</small>
        <Button primary type="submit" disabled={mutation.busy}>{mutation.busy ? 'Starting…' : 'Start replying'}</Button>
      </form>}
      {!starting && position === 'auto' && <p className="chat-replies-note">{thread?.instruction && <>What you asked: {thread.instruction}<br /></>}{thread?.sent ?? 0} {thread?.sent === 1 ? 'message' : 'messages'} sent as you. You can write here too.</p>}
      {!starting && thread?.detail && <p className="chat-replies-note">{thread.detail}</p>}
      {mutation.status && <p className="chat-error" role="alert">{mutation.status}</p>}
    </div>}
  </span>;
}
