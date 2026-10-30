import { useRef, useState } from 'react';
import { workspaceUrl } from '../../workspace-context';
import { useMutation } from '../requests';
import { Button, Dialog } from '../ui';
import { chatRequest, type Message } from './model';
import './message-actions.css';

/** Must match the server's fixed set (postgres/message_actions.rs). */
export const REACTIONS = ['👍', '✅', '👀', '🙏', '🎉', '❤️', '😂', '🔥', '👏', '🤔', '😮', '💯'] as const;
const QUICK = REACTIONS.slice(0, 3);

const path = (message: Message, action: string) => `/chats/${message.chat}/messages/${message.id}/${action}`;

/** Reactions are expressions only; they never approve anything or ask an agent to act. */
export function useReactions(message: Message) {
  const mutation = useMutation();
  const toggle = (emoji: string) => void mutation.run(async () => {
    const on = !message.reactions?.some(reaction => reaction.emoji === emoji && reaction.mine);
    await chatRequest(path(message, 'reactions'), { emoji, on });
  });
  return { ...mutation, toggle };
}

export function ReactionBar({ message, reactions }: { message: Message; reactions: ReturnType<typeof useReactions> }) {
  const [picking, setPicking] = useState(false);
  if (!message.reactions?.length) return null;
  return <div className="chat-reactions" aria-label="Reactions">
    {message.reactions.map(reaction => <button type="button" key={reaction.emoji} className="chat-reaction" aria-pressed={reaction.mine}
      disabled={reactions.busy} aria-label={`${reaction.emoji} ${reaction.count}${reaction.mine ? ', including you' : ''}`}
      onClick={() => reactions.toggle(reaction.emoji)}><span aria-hidden="true">{reaction.emoji}</span> {reaction.count}</button>)}
    <ReactionPicker open={picking} setOpen={setPicking} choose={reactions.toggle} busy={reactions.busy} />
    {reactions.status && <span className="chat-help" role="alert">{reactions.status}</span>}
  </div>;
}

function ReactionPicker({ open, setOpen, choose, busy }: { open: boolean; setOpen: (open: boolean) => void; choose: (emoji: string) => void; busy: boolean }) {
  return <span className="chat-reaction-picker">
    <button type="button" className="chat-message-tool" aria-label="Add reaction" aria-expanded={open} title="Add reaction" onClick={() => setOpen(!open)}><SmileIcon /></button>
    {open && <span className="chat-reaction-menu" role="group" aria-label="Choose a reaction">
      {REACTIONS.map(emoji => <button type="button" key={emoji} disabled={busy} onClick={() => { choose(emoji); setOpen(false); }}>{emoji}</button>)}
    </span>}
  </span>;
}

/** Appears on hover or focus: quick reactions, reply in thread and More. */
export function MessageToolbar({ message, own, reactions, openThread, edit, remove }: {
  message: Message; own: boolean; reactions: ReturnType<typeof useReactions>;
  openThread?: () => void; edit: () => void; remove: () => void;
}) {
  const [picking, setPicking] = useState(false);
  const [more, setMore] = useState(false);
  const [copied, setCopied] = useState(false);
  const [upward, setUpward] = useState(false);
  const bar = useRef<HTMLDivElement>(null);
  // Menus open upward in the lower half so the composer never hides them.
  const place = () => setUpward((bar.current?.getBoundingClientRect().top ?? 0) > window.innerHeight / 2);
  // Reactions and links need the stored message; a reply only needs the thread.
  const stored = Boolean(message.chat);
  const deliverable = stored && (message.state === 'sent' || message.state === 'complete');
  const copy = async () => {
    await navigator.clipboard?.writeText(new URL(workspaceUrl(`/?view=chat&conversation=${message.chat}&message=${message.id}`), location.origin).href);
    setCopied(true); setMore(false);
  };
  return <div ref={bar} className={`chat-message-toolbar${upward ? ' opens-up' : ''}`} role="toolbar" aria-label="Message actions">
    {deliverable && QUICK.map(emoji => <button type="button" key={emoji} className="chat-message-tool" disabled={reactions.busy} aria-label={`React ${emoji}`} title={`React ${emoji}`} onClick={() => reactions.toggle(emoji)}>{emoji}</button>)}
    {deliverable && <ReactionPicker open={picking} setOpen={open => { place(); setPicking(open); }} choose={reactions.toggle} busy={reactions.busy} />}
    {openThread && <button type="button" className="chat-message-tool" aria-label="Reply in thread" title="Reply in thread" onClick={openThread}>↩</button>}
    {stored && <span className="chat-message-more">
      <button type="button" className="chat-message-tool" aria-label="More actions" aria-expanded={more} title={copied ? 'Link copied' : 'More'} onClick={() => { place(); setMore(!more); }}>⋯</button>
      {more && <span className="chat-message-menu" role="menu">
        {own && <button type="button" role="menuitem" onClick={() => { setMore(false); edit(); }}>Edit message</button>}
        <button type="button" role="menuitem" onClick={() => void copy()}>Copy link</button>
        {own && <button type="button" role="menuitem" className="danger" onClick={() => { setMore(false); remove(); }}>Delete message</button>}
      </span>}
    </span>}
  </div>;
}

export function EditMessage({ message, done }: { message: Message; done: () => void }) {
  const [text, setText] = useState(message.content);
  const mutation = useMutation();
  const save = () => void mutation.run(async () => {
    if (text.trim() === message.content.trim()) { done(); return; }
    await chatRequest(path(message, 'edit'), { content: text });
    done();
  });
  return <form className="chat-message-edit" onSubmit={event => { event.preventDefault(); save(); }}>
    <textarea aria-label="Edit message" autoFocus rows={Math.min(8, text.split('\n').length + 1)} maxLength={12000} value={text} disabled={mutation.busy}
      onChange={event => setText(event.target.value)}
      onKeyDown={event => { if (event.key === 'Escape') done(); if (event.key === 'Enter' && !event.shiftKey) { event.preventDefault(); save(); } }} />
    <div><Button onClick={done} disabled={mutation.busy}>Cancel</Button><Button primary type="submit" disabled={mutation.busy || !text.trim()}>{mutation.busy ? 'Saving…' : 'Save'}</Button></div>
    {mutation.status && <p className="chat-error" role="alert">{mutation.status}</p>}
    <p className="chat-help">Agent work this message already started continues with the original words.</p>
  </form>;
}

export function DeleteMessage({ message, close }: { message: Message; close: () => void }) {
  const mutation = useMutation();
  return <Dialog title="Delete message?" busy={mutation.busy} status={mutation.status} close={close}
    footer={<><Button onClick={close} disabled={mutation.busy}>Cancel</Button><Button primary className="danger" disabled={mutation.busy}
      onClick={() => void mutation.run(async () => { await chatRequest(path(message, 'delete'), {}); close(); })}>{mutation.busy ? 'Deleting…' : 'Delete'}</Button></>}>
    <p>Everyone in this conversation will see “Message deleted”. Its text and files are removed.</p>
    <p className="chat-help">If the message asked an agent to do something that has not finished, that work stops.</p>
  </Dialog>;
}

function SmileIcon() {
  return <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true">
    <circle cx="12" cy="12" r="9" /><path d="M8.5 14.5a4 4 0 0 0 7 0M9 9.5h.01M15 9.5h.01" />
  </svg>;
}
