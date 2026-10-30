import { useEffect, useRef, useState } from 'react';
import { acknowledge } from './live';
import { chatRequest, type Message } from './model';
export function useSeen(message: Message, own: boolean) {
  const element = useRef<HTMLElement>(null);
  useEffect(() => {
    if (own || !message.chat || !element.current || !['sent', 'complete'].includes(message.state)) return;
    let visible = false, timer: number | undefined;
    const schedule = () => {
      clearTimeout(timer);
      if (visible && !document.hidden && document.hasFocus()) timer = window.setTimeout(() => acknowledge(message.chat!, [message], true), 700);
    };
    const observer = new IntersectionObserver(entries => { visible = entries[0].isIntersecting && entries[0].intersectionRatio >= 0.6; schedule(); }, { threshold: [0, 0.6] });
    observer.observe(element.current);
    window.addEventListener('focus', schedule); window.addEventListener('blur', schedule); document.addEventListener('visibilitychange', schedule);
    return () => { clearTimeout(timer); observer.disconnect(); window.removeEventListener('focus', schedule); window.removeEventListener('blur', schedule); document.removeEventListener('visibilitychange', schedule); };
  }, [message.id, message.chat, message.state, own]);
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
