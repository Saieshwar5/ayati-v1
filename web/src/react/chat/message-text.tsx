// A person's message text: long ones fold behind "Show more", and the first web
// link gets a preview card (title, site, description; never a picture) read
// once the message is on screen.
import { useEffect, useRef, useState } from 'react';
import { messageHtml } from '../../markdown';
import { chatRequest, type Message } from './model';

/** Longer than this (characters or lines) folds until "Show more". */
const LONG_CHARS = 1200, LONG_LINES = 14;
export const isLong = (text: string) => text.length > LONG_CHARS || text.split('\n').length > LONG_LINES;
const hasLink = (text: string) => /https?:\/\/\S/.test(text);

export type LinkPreview = { url: string; title: string; site: string; description?: string | null };
/** One read per message and revision on this page, shared by every render. */
const previews = new Map<string, Promise<LinkPreview | null>>();

export function PersonText({ message, text }: { message: Message; text: string }) {
  const [open, setOpen] = useState(false);
  const long = isLong(text);
  return <>
    <div className={`message-markdown${long && !open ? ' message-folded' : ''}`} dangerouslySetInnerHTML={{ __html: messageHtml(text) }} />
    {long && <button type="button" className="chat-thread-link message-fold" aria-expanded={open} onClick={() => setOpen(!open)}>{open ? 'Show less' : 'Show more'}</button>}
    {message.chat && message.kind === 'user' && message.state === 'sent' && hasLink(text) && <LinkCard message={message} />}
  </>;
}

function LinkCard({ message }: { message: Message }) {
  const [preview, setPreview] = useState<LinkPreview | null>(null);
  const card = useRef<HTMLDivElement>(null);
  const key = `${message.chat}/${message.id}/${message.revision ?? 0}`;
  useEffect(() => {
    const node = card.current;
    if (!node) return;
    let current = true;
    const read = () => {
      let pending = previews.get(key);
      if (!pending) {
        pending = chatRequest<{ preview: LinkPreview | null }>(`/chats/${message.chat}/messages/${message.id}/link-preview`).then(answer => answer.preview, () => null);
        previews.set(key, pending);
      }
      void pending.then(found => { if (current) setPreview(found); });
    };
    // Read only once the message scrolls into view.
    const watcher = new IntersectionObserver(entries => { if (entries.some(entry => entry.isIntersecting)) { watcher.disconnect(); read(); } });
    watcher.observe(node);
    return () => { current = false; watcher.disconnect(); };
  }, [key, message.chat, message.id]);
  return <div ref={card} className="message-link-card-slot">{preview && /^https?:\/\//i.test(preview.url) && <a className="message-link-card" href={preview.url} target="_blank" rel="noopener noreferrer">
    <small>{preview.site}</small>
    <strong>{preview.title}</strong>
    {preview.description && <span>{preview.description}</span>}
  </a>}</div>;
}
