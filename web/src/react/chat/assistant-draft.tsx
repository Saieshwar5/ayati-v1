import { useEffect, useRef, useState } from 'react';
import { Button } from '../ui';
import { errorMessage } from '../requests';
import { chatRequest } from './model';
import { subscribeSent } from './outbox';
import { ThreadIcon } from './assistant-threads';
import { useThreads } from './assistant-threads-model';

type Draft = { reply: string; state: 'writing' | 'ready' | 'failed'; text: string };

/** Above the message box in any conversation but your assistant's own: ask
 * your assistant for a reply. The draft is shown to you only; nothing is sent
 * until you put it in the message box and send it yourself (schema 129). */
export function AssistantDraft({ chat, thread, name, instruction, use, sent }: {
  chat: string; thread?: string; name: string;
  /** What is in the message box now: sent along as what you want to say. */
  instruction: string;
  use: (text: string) => void;
  /** Lets the conversation tell this component which message a used draft became. */
  sent: { current?: (message: string) => void };
}) {
  const { assistant, room } = useThreads();
  const [draft, setDraft] = useState<Draft>();
  const [error, setError] = useState('');
  const pending = useRef(false);
  const used = useRef<{ reply: string; message?: string }>(undefined);
  const waiting = draft?.state === 'writing' ? draft.reply : undefined;
  // While the assistant writes, read the draft every two seconds, for two minutes.
  useEffect(() => {
    if (!waiting) return;
    const abort = new AbortController(); let tries = 0;
    const timer = window.setInterval(async () => {
      if (++tries > 60) { setDraft(previous => previous?.reply === waiting ? { ...previous, state: 'failed' } : previous); return; }
      try {
        const result = await chatRequest<{ state: string; text: string }>(`/assistant/drafts/${encodeURIComponent(waiting)}`, undefined, abort.signal);
        if (abort.signal.aborted) return;
        if (result.state === 'complete') setDraft(previous => previous?.reply === waiting ? { reply: waiting, state: 'ready', text: result.text } : previous);
        else if (!['queued', 'streaming'].includes(result.state)) setDraft(previous => previous?.reply === waiting ? { ...previous, state: 'failed' } : previous);
      } catch { /* The next read tries again. */ }
    }, 2000);
    return () => { abort.abort(); window.clearInterval(timer); };
  }, [waiting]);
  // Once your message with a used draft is sent, keep the private record of who wrote it.
  useEffect(() => {
    sent.current = message => { if (used.current) used.current.message = message; };
    const unsubscribe = subscribeSent((_, message) => {
      const record = used.current;
      if (!record?.message || message?.id !== record.message) return;
      used.current = undefined;
      void chatRequest(`/assistant/drafts/${encodeURIComponent(record.reply)}/used`, { message: record.message }).catch(() => undefined);
    });
    return () => { sent.current = undefined; unsubscribe(); };
  }, [sent]);
  if (!assistant || chat === room) return null;
  const ask = async () => {
    if (pending.current) return;
    pending.current = true; setError('');
    try {
      const result = await chatRequest<{ reply: string }>('/assistant/drafts', { id: crypto.randomUUID(), chat, thread: thread ?? null, instruction: instruction.trim() });
      setDraft({ reply: result.reply, state: 'writing', text: '' });
    } catch (e) { setError(errorMessage(e)); }
    finally { pending.current = false; }
  };
  if (!draft) return <div className="chat-draft-ask">
    <Button onClick={() => void ask()} title={`${name} writes a reply for you to check. Nothing is sent.`}><ThreadIcon />Draft with {name}</Button>
    {error && <span className="chat-error" role="alert">{error}</span>}
  </div>;
  return <div className="chat-draft" role="status" aria-label={`Draft from ${name}`}>
    <header><ThreadIcon /><strong>{draft.state === 'writing' ? `${name} is writing a draft…` : draft.state === 'ready' ? `Draft from ${name}` : `${name} could not write a draft`}</strong><small>Only you see this</small></header>
    {draft.state === 'ready' && <p>{draft.text}</p>}
    <div>
      {draft.state === 'ready' && <Button primary onClick={() => { used.current = { reply: draft.reply }; use(draft.text); setDraft(undefined); }}>Use draft</Button>}
      {draft.state === 'failed' && <Button onClick={() => { setDraft(undefined); void ask(); }}>Try again</Button>}
      <Button onClick={() => setDraft(undefined)}>{draft.state === 'writing' ? 'Cancel' : 'Discard'}</Button>
    </div>
  </div>;
}
