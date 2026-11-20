import { useEffect, useRef, useState } from 'react';
import { acknowledge } from './live';
import { attentive, subscribeAttention } from './reading-attention';
import { chatRequest, type Message } from './model';

/** How much of a message, or of the area it is read in, must show before it counts as read. */
const SHARE = 0.6;
/** The shares of a message at which the browser reports its visibility. Each is
 * under 1/0.6 of the next, so however tall a message is, one of them lies between
 * "it fills 60% of the reading area" and "it fills all of it". */
export const SEEN_STEPS = [0, ...Array.from({ length: 12 }, (_, step) => SHARE / 1.5 ** step)];

/** Whether a message is shown enough to count as read: most of it is on screen
 * or, for one taller than the area it scrolls in, it fills most of that area.
 * Without the second rule a long message can never be read, and its unread
 * count never goes. */
export function shownEnough(share: number, visibleHeight: number, areaHeight: number): boolean {
  return share >= SHARE || (areaHeight > 0 && visibleHeight >= SHARE * areaHeight);
}

/** What a message scrolls in; nothing when only the window scrolls. */
function scrollArea(node: HTMLElement): HTMLElement | null {
  for (let parent = node.parentElement; parent; parent = parent.parentElement) {
    const overflow = window.getComputedStyle(parent).overflowY;
    if (overflow === 'auto' || overflow === 'scroll') return parent;
  }
  return null;
}

/** Marks someone else's message read once it has been shown for a moment while
 * the person is looking at the page (reading-attention.ts). `read` hears which
 * message, so a thread's count of new replies can drop at once; pass a stable function. */
export function useSeen(message: Message, own: boolean, read?: (id: string) => void) {
  const element = useRef<HTMLElement>(null);
  useEffect(() => {
    const node = element.current;
    if (own || !message.chat || !node || !['sent', 'complete'].includes(message.state)) return;
    // Read once is enough: focus and renewed use must not send it again.
    let visible = false, done = false, timer: number | undefined, area: HTMLElement | null | undefined;
    const schedule = () => {
      clearTimeout(timer);
      if (done || !visible || !attentive()) return;
      timer = window.setTimeout(() => {
        // Still looked at: an unfocused window counts only while it is being used.
        if (!attentive()) return;
        done = true; acknowledge(message.chat!, [message], true); read?.(message.id);
      }, 700);
    };
    const observer = new IntersectionObserver(entries => {
      const entry = entries[entries.length - 1];
      visible = entry.isIntersecting && entry.intersectionRatio >= SHARE;
      // On screen but not mostly so: it may be taller than the area it is read in.
      if (entry.isIntersecting && !visible) {
        if (area === undefined) area = scrollArea(node);
        visible = shownEnough(entry.intersectionRatio, entry.intersectionRect.height, area?.clientHeight || window.innerHeight);
      }
      schedule();
    }, { threshold: SEEN_STEPS });
    observer.observe(node);
    const unsubscribe = subscribeAttention(schedule);
    window.addEventListener('focus', schedule); window.addEventListener('blur', schedule); document.addEventListener('visibilitychange', schedule);
    return () => { clearTimeout(timer); observer.disconnect(); unsubscribe(); window.removeEventListener('focus', schedule); window.removeEventListener('blur', schedule); document.removeEventListener('visibilitychange', schedule); };
  }, [message.id, message.chat, message.state, own, read]);
  return element;
}
type Receipt = { name: string; agent?: boolean; delivered_at: number; seen_at: number | null };
export function MessageReceipt({ message }: { message: Message }) {
  const [details, setDetails] = useState<Receipt[]>();
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const label = message.seen ? `✓✓ Seen by ${message.seen}` : message.delivered ? `✓✓ Delivered to ${message.delivered}` : '✓ Sent';
  return <details className="chat-receipt" onToggle={event => {
    if (!event.currentTarget.open || busy || !message.chat) return;
    setBusy(true); setError('');
    void chatRequest<{ receipts: Receipt[] }>(`/chats/${message.chat}/receipts/${message.id}`).then(data => setDetails(data.receipts)).catch(() => { setDetails(undefined); setError('Receipt details unavailable.'); }).finally(() => setBusy(false));
  }}>
    <summary aria-label={label} title={label} className={message.seen ? 'chat-seen' : ''}>{message.seen || message.delivered ? '✓✓' : '✓'}</summary>
    {busy ? <small>Loading…</small> : error ? <small>{error}</small> : details?.length ? details.map((r, i) => <small key={i}>{r.name}{r.agent ? ' · AI' : ''} · {r.seen_at ? 'Seen' : 'Delivered'} {new Date((r.seen_at ?? r.delivered_at) * 1000).toLocaleTimeString()}</small>) : <small>No recipient has acknowledged this message yet.</small>}
  </details>;
}
