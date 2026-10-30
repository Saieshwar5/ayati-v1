import type { ReactNode } from 'react';
import { decodeMentions } from './mentions';
import type { Message } from './model';
import './quoted-message.css';

/** Who wrote a quoted message and a one-line preview of what they said. */
export function quotePreview(message: Message | undefined, agentName: (id: string) => string | undefined) {
  if (!message) return { name: 'Earlier message', text: 'Not loaded in this view' };
  const name = message.kind === 'agent' ? agentName(message.author) ?? message.name ?? 'Agent' : message.name ?? 'Former member';
  const raw = message.state === 'deleted' ? 'Message deleted' : decodeMentions(message.progress?.text || message.content).text;
  // Drop markdown markers only; underscores and brackets can be real text.
  const text = raw.replace(/[#*`>|]/g, '').replace(/\s+/g, ' ').trim() || 'Shared files';
  return { name, text: text.length > 140 ? `${text.slice(0, 139)}…` : text };
}

/** The quote shown inside a reply; activating it scrolls to the original when it is loaded. */
export function QuotedMessage({ id, name, text }: { id: string; name: string; text: string }) {
  const jump = (event: { currentTarget: HTMLElement }) => {
    const original = event.currentTarget.closest('[role=log]')?.querySelector<HTMLElement>(`[data-message-id="${CSS.escape(id)}"]`);
    original?.scrollIntoView({ block: 'center', behavior: 'smooth' });
    original?.focus({ preventScroll: true });
  };
  return <button type="button" className="chat-quote" aria-label={`Replying to ${name}: ${text}`} onClick={jump}>
    <strong>{name}</strong><span>{text}</span>
  </button>;
}

/** The quote above the composer while a reply is being written. */
export function ReplyingTo({ name, text, cancel }: { name: string; text: string; cancel: () => void }): ReactNode {
  return <div className="chat-replying" role="status">
    <div><strong>Replying to {name}</strong><span>{text}</span></div>
    <button type="button" className="chat-message-tool" aria-label="Cancel reply" title="Cancel reply" onClick={cancel}>×</button>
  </div>;
}
