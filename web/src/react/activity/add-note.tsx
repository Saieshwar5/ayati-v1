import { useId, useRef, useState } from 'react';
import { Button } from '../ui';
import { useMutation } from '../requests';

/** Add to running work (Phase 4 AC6): what the person writes reaches the agent at
 * its next step, instead of waiting behind it as a new request. */
export function AddNote({ label, send, sent }: { label: string; send: (text: string) => Promise<unknown>; sent?: () => void }) {
  const [text, setText] = useState('');
  const m = useMutation();
  const field = useId();
  const busy = useRef(false);
  const submit = () => {
    if (busy.current || !text.trim()) return;
    busy.current = true;
    void m.run(async () => {
      try { await send(text.trim()); setText(''); m.setStatus('Added. The agent reads it at its next step.'); sent?.(); }
      finally { busy.current = false; }
    });
  };
  return <form className="activity-add-note" onSubmit={event => { event.preventDefault(); submit(); }}>
    <label htmlFor={field}>{label}</label>
    <textarea id={field} rows={2} maxLength={2000} value={text} disabled={m.busy} placeholder="Also use the new price list"
      onChange={event => setText(event.target.value)} />
    <Button type="submit" disabled={m.busy || !text.trim()}>Add</Button>
    {m.status && <small role="status">{m.status}</small>}
  </form>;
}
