import { useEffect, useRef, useState, type ReactNode } from 'react';
import { Button } from '../ui';
import { errorMessage } from '../requests';
import { chatRequest } from './model';
import { subscribeSent } from './outbox';
import { AssistantMark } from './assistant-threads';
import { discardReply, useThreads, type WaitingReply } from './assistant-threads-model';
import { decodeMentions } from './mentions';

type Draft = { reply: string; state: 'writing' | 'ready' | 'failed'; /** What was in the message box before. */ before: string;
  /** The thread a reply was written in on its own (Ask me first), as opposed to one you asked for. */
  thread?: string };

/** Replies written on their own that this page has already put in a message box,
 * so one is offered once and never over what you typed afterwards. */
const offered = new Set<string>();

/** What you typed without the mention of your assistant: what you want to say. */
export function wordsFor(source: string, assistant: string): string {
  const { text, mentions } = decodeMentions(source);
  let words = text;
  for (const mention of mentions.filter(item => item.kind === 'agent' && item.id === assistant).reverse()) words = words.slice(0, mention.start) + words.slice(mention.end);
  return words.replace(/[ \t]+/g, ' ').trim();
}
/** Whether the message box mentions your own assistant: such a message asks it and is never posted. */
export const mentionsAssistant = (source: string, assistant?: string) =>
  Boolean(assistant) && decodeMentions(source).mentions.some(item => item.kind === 'agent' && item.id === assistant);

/** Your assistant in the message box of any conversation but its own (schemas
 * 129 and 141). You ask for a reply with the button or by mentioning it; on Ask
 * me first it writes one on its own. Either way the reply is written into the
 * box, for you only, and nothing is sent until you send it yourself. Returns
 * the button for the box, the line shown on top of it, and `ask`. */
export function useAssistantDraft({ chat, thread, name, value, change, sent, waiting, disabled = false }: {
  chat: string; thread?: string; name: string;
  /** The message box: what you typed is sent along as what you want to say. */
  value: string; change: (text: string) => void;
  /** Lets the conversation tell this hook which message a draft became. */
  sent: { current?: (message: string) => void };
  /** On Ask me first: the reply your assistant wrote on its own in `thread`. */
  waiting?: { thread: string } & WaitingReply;
  disabled?: boolean;
}): { tool: ReactNode; notice: ReactNode; ask?: (message?: string) => void; asking: boolean } {
  const { assistant, room, reload } = useThreads();
  // A reply already offered and still in the box after coming back to this chat.
  const kept = () => waiting && offered.has(waiting.reply) && value.trim() ? { reply: waiting.reply, state: 'ready' as const, before: '', thread: waiting.thread } : undefined;
  const [draft, setDraft] = useState<Draft | undefined>(kept);
  const [error, setError] = useState('');
  const pending = useRef(false);
  const used = useRef<{ reply: string; message?: string } | undefined>(draft && { reply: draft.reply });
  const box = useRef({ value, change }); box.current = { value, change };
  const writing = draft?.state === 'writing' ? draft.reply : undefined;
  // The draft is still what the box holds: it was written in and not cleared.
  const shown = draft?.state === 'ready' && Boolean(value.trim());
  const visible = useRef(shown); visible.current = shown;
  const ready = waiting?.state === 'complete' && waiting.text.trim() ? waiting : undefined;
  const offer = (reply: NonNullable<typeof ready>) => {
    offered.add(reply.reply); used.current = { reply: reply.reply };
    setDraft({ reply: reply.reply, state: 'ready', before: box.current.value, thread: reply.thread });
    box.current.change(reply.text);
  };
  // A reply written on its own goes into the box once, and only while the box is
  // empty: the server's reply is copied into your draft, never over your words.
  const empty = !value.trim();
  useEffect(() => {
    if (ready && empty && !draft && !offered.has(ready.reply)) offer(ready);
  }, [ready?.reply, empty, Boolean(draft)]);
  // While the assistant writes, read the draft every two seconds, for two minutes.
  useEffect(() => {
    if (!writing) return;
    const abort = new AbortController(); let tries = 0;
    const fail = () => setDraft(previous => previous?.reply === writing ? { ...previous, state: 'failed' } : previous);
    const timer = window.setInterval(async () => {
      if (++tries > 60) { fail(); return; }
      try {
        const result = await chatRequest<{ state: string; text: string }>(`/assistant/drafts/${encodeURIComponent(writing)}`, undefined, abort.signal);
        if (abort.signal.aborted) return;
        if (result.state === 'complete') {
          used.current = { reply: writing }; box.current.change(result.text);
          setDraft(previous => previous?.reply === writing ? { ...previous, state: 'ready' } : previous);
        } else if (!['queued', 'streaming'].includes(result.state)) fail();
      } catch { /* The next read tries again. */ }
    }, 2000);
    return () => { abort.abort(); window.clearInterval(timer); };
  }, [writing]);
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
      // The record also tells the thread its reply waits no more.
      void chatRequest(`/assistant/drafts/${encodeURIComponent(record.reply)}/used`, { message: record.message }).then(reload, () => undefined);
    });
    return () => { sent.current = undefined; unsubscribe(); };
  }, [sent]);
  if (!assistant || chat === room) return { tool: null, notice: null, asking: false };
  const asking = mentionsAssistant(value, assistant);
  const ask = async (message?: string) => {
    if (pending.current || writing) return;
    pending.current = true; setError(''); used.current = undefined;
    const before = box.current.value;
    try {
      const result = await chatRequest<{ reply: string }>('/assistant/drafts',
        { id: crypto.randomUUID(), chat, thread: thread ?? null, instruction: wordsFor(before, assistant), ...(message ? { message } : {}) });
      setDraft({ reply: result.reply, state: 'writing', before });
    } catch (e) { setError(errorMessage(e)); }
    finally { pending.current = false; }
  };
  const discard = () => {
    if (!draft) return;
    change(draft.before); used.current = undefined; setDraft(undefined);
    if (draft.thread) void discardReply(draft.thread).then(reload, () => undefined);
  };
  const tool = <Button className="composer-draft" aria-label={`Draft with ${name}`} disabled={disabled || Boolean(writing)}
    title={`Draft with ${name}. It writes a reply for you to check. Nothing is sent.`} onClick={() => void ask()}><AssistantMark size={17} /></Button>;
  const notice = error ? <div className="composer-notice" role="alert"><AssistantMark /><span>{error}</span><Button onClick={() => setError('')}>Dismiss</Button></div>
    : draft?.state === 'writing' ? <div className="composer-notice" role="status"><AssistantMark /><strong>{name} is writing a draft…</strong><span>Only you see this.</span><Button onClick={() => setDraft(undefined)}>Cancel</Button></div>
    : draft?.state === 'failed' ? <div className="composer-notice" role="status"><AssistantMark /><strong>{name} could not write a draft</strong><span /><Button onClick={() => { setDraft(undefined); void ask(); }}>Try again</Button><Button onClick={() => setDraft(undefined)}>Dismiss</Button></div>
    : shown ? <div className="composer-notice" role="status" aria-label={`${draft.thread ? 'Reply' : 'Draft'} from ${name}`}><AssistantMark /><strong>{draft.thread ? 'Reply' : 'Draft'} from {name}</strong><span>Only you see this. Edit it, then send.</span><Button onClick={discard}>Discard</Button></div>
    : asking ? <div className="composer-notice" role="status"><AssistantMark /><strong>Only you see this.</strong><span>{name} writes the reply here. Nothing is sent.</span></div>
    : ready && !offered.has(ready.reply) ? <div className="composer-notice" role="status"><AssistantMark /><strong>{name} has a reply ready</strong><span>It replaces what you typed. Discard puts your words back.</span><Button onClick={() => offer(ready)}>Use it</Button></div>
    : null;
  return { tool, notice, ask: message => void ask(message), asking };
}
