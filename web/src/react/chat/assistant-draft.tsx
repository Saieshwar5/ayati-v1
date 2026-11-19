import { useEffect, useRef, useState, type ReactNode } from 'react';
import { Button } from '../ui';
import { errorMessage } from '../requests';
import { chatRequest } from './model';
import { subscribeSent } from './outbox';
import { AssistantMark } from './assistant-threads';
import { useThreads } from './assistant-threads-model';

type Draft = { reply: string; state: 'writing' | 'ready' | 'failed'; /** What was in the message box when you asked. */ before: string };

/** In the message box of any conversation but your assistant's own: ask your
 * assistant for a reply. The draft is written into the box, for you only;
 * nothing is sent until you send it yourself (schema 129). Returns the button
 * for the box and the line shown on top of it. */
export function useAssistantDraft({ chat, thread, name, value, change, sent, disabled = false }: {
  chat: string; thread?: string; name: string;
  /** The message box: what you typed is sent along as what you want to say. */
  value: string; change: (text: string) => void;
  /** Lets the conversation tell this hook which message a draft became. */
  sent: { current?: (message: string) => void };
  disabled?: boolean;
}): { tool: ReactNode; notice: ReactNode } {
  const { assistant, room } = useThreads();
  const [draft, setDraft] = useState<Draft>();
  const [error, setError] = useState('');
  const pending = useRef(false);
  const used = useRef<{ reply: string; message?: string }>(undefined);
  const box = useRef({ value, change }); box.current = { value, change };
  const waiting = draft?.state === 'writing' ? draft.reply : undefined;
  // The draft is still what the box holds: it was written in and not cleared.
  const shown = draft?.state === 'ready' && Boolean(value.trim());
  const visible = useRef(shown); visible.current = shown;
  // While the assistant writes, read the draft every two seconds, for two minutes.
  useEffect(() => {
    if (!waiting) return;
    const abort = new AbortController(); let tries = 0;
    const fail = () => setDraft(previous => previous?.reply === waiting ? { ...previous, state: 'failed' } : previous);
    const timer = window.setInterval(async () => {
      if (++tries > 60) { fail(); return; }
      try {
        const result = await chatRequest<{ state: string; text: string }>(`/assistant/drafts/${encodeURIComponent(waiting)}`, undefined, abort.signal);
        if (abort.signal.aborted) return;
        if (result.state === 'complete') {
          used.current = { reply: waiting }; box.current.change(result.text);
          setDraft(previous => previous?.reply === waiting ? { ...previous, state: 'ready' } : previous);
        } else if (!['queued', 'streaming'].includes(result.state)) fail();
      } catch { /* The next read tries again. */ }
    }, 2000);
    return () => { abort.abort(); window.clearInterval(timer); };
  }, [waiting]);
  // Once your message with a draft is sent, keep the private record of who wrote it.
  useEffect(() => {
    sent.current = message => {
      if (used.current && visible.current) used.current.message = message; else used.current = undefined;
      setDraft(undefined);
    };
    const unsubscribe = subscribeSent((_, message) => {
      const record = used.current;
      if (!record?.message || message?.id !== record.message) return;
      used.current = undefined;
      void chatRequest(`/assistant/drafts/${encodeURIComponent(record.reply)}/used`, { message: record.message }).catch(() => undefined);
    });
    return () => { sent.current = undefined; unsubscribe(); };
  }, [sent]);
  if (!assistant || chat === room) return { tool: null, notice: null };
  const ask = async () => {
    if (pending.current) return;
    pending.current = true; setError(''); used.current = undefined;
    const before = box.current.value;
    try {
      const result = await chatRequest<{ reply: string }>('/assistant/drafts', { id: crypto.randomUUID(), chat, thread: thread ?? null, instruction: before.trim() });
      setDraft({ reply: result.reply, state: 'writing', before });
    } catch (e) { setError(errorMessage(e)); }
    finally { pending.current = false; }
  };
  const discard = () => { if (draft) change(draft.before); used.current = undefined; setDraft(undefined); };
  const tool = <Button className="composer-draft" aria-label={`Draft with ${name}`} disabled={disabled || draft?.state === 'writing'}
    title={`Draft with ${name}. It writes a reply for you to check. Nothing is sent.`} onClick={() => void ask()}><AssistantMark size={17} /></Button>;
  const notice = error ? <div className="composer-notice" role="alert"><AssistantMark /><span>{error}</span><Button onClick={() => setError('')}>Dismiss</Button></div>
    : draft?.state === 'writing' ? <div className="composer-notice" role="status"><AssistantMark /><strong>{name} is writing a draft…</strong><span>Only you see this.</span><Button onClick={() => setDraft(undefined)}>Cancel</Button></div>
    : draft?.state === 'failed' ? <div className="composer-notice" role="status"><AssistantMark /><strong>{name} could not write a draft</strong><span /><Button onClick={() => { setDraft(undefined); void ask(); }}>Try again</Button><Button onClick={() => setDraft(undefined)}>Dismiss</Button></div>
    : shown ? <div className="composer-notice" role="status" aria-label={`Draft from ${name}`}><AssistantMark /><strong>Draft from {name}</strong><span>Only you see this. Edit it, then send.</span><Button onClick={discard}>Discard</Button></div>
    : null;
  return { tool, notice };
}
