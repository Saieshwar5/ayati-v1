// What the conversation shows of a thread (schema 115): "4 replies · last
// 10:52" under its first message, and on a reply also sent to the
// conversation, which thread it belongs to.

/** 10:52 today; "13 Nov" on another day. */
export function replyTime(at: number, now = new Date()) {
  const when = new Date(at * 1000);
  return when.toDateString() === now.toDateString()
    ? when.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
    : when.toLocaleDateString([], { day: 'numeric', month: 'short' });
}

export function ThreadSummary({ replies, last, open }: { replies: number; last?: number | null; open?: () => void }) {
  const count = `${replies} ${replies === 1 ? 'reply' : 'replies'}`;
  const label = last ? `${count} · last ${replyTime(last)}` : count;
  return <button type="button" className="chat-thread-link chat-thread-count" aria-label={`${label}. Open thread`} title="Open thread" onClick={open}>{label}</button>;
}

/** A thread reply that was also sent to the conversation. */
export function InThreadLine({ root, open }: { root?: string; open?: () => void }) {
  return <p className="chat-in-thread">
    <span>Replied in a thread{root ? ': ' : ''}</span>{root && <q>{root}</q>}
    {open && <button type="button" className="chat-thread-link" onClick={open}>View thread</button>}
  </p>;
}
