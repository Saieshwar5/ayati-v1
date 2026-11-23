import { useEffect, useRef, useState, type ReactNode } from 'react';
import { Button } from '../ui';
import { errorMessage } from '../requests';
import { chatRequest, type Message } from './model';
import { subscribeSent } from './outbox';
import { AssistantMark } from './assistant-threads';
import { discardReply, useThreads, type WaitingReply } from './assistant-threads-model';
import type { Mention } from './mentions';
import { AskCard } from './assistant-ask-card';
import { answersLatest, assistantRequest, keepAsked, latestForYou, loadAsked, mentionAssistant, mentionTyped, namesAssistant, type Asked } from './assistant-ask-model';

export type AssistantAsk = {
  /** Your assistant, offered first in this message box's @ list. */
  mention?: Mention;
  /** The message box mentions your assistant: sending asks it and posts nothing. */
  asking: boolean;
  /** Asks your assistant what the message box says; true once it was asked. */
  ask: () => Promise<boolean>;
  /** The button beside Attach, the line on top of the message box, and the draft above it. */
  tool: ReactNode; notice: ReactNode; card: ReactNode;
};
const none: AssistantAsk = { asking: false, ask: async () => false, tool: null, notice: null, card: null };

type Request = { instruction: string; change?: string; message?: string; previous?: string; quote?: string };

/** Ask your own assistant from the message box of a conversation or thread
 * (2026-11-21): mention it and say what you want. It writes a reply where only
 * you see it; you send that reply as your own message, edit it or discard it.
 * What you asked is never posted. Not in your assistant's own chat, where you
 * are already talking to it. */
export function useAssistantAsk({ chat, thread, where, direct, assistant, viewer, value, change, messages, quote, post, sent, waiting, focusBox, disabled = false }: {
  chat: string; thread?: string;
  /** Where a message sent from here goes: "in #sales", "to Meera Iyer". */
  where: string;
  /** A direct chat: the other side's latest message is the one meant for you. */
  direct: boolean;
  assistant?: { id: string; name: string; avatar?: string | null };
  viewer?: string;
  /** The message box. */
  value: string; change: (text: string) => void;
  /** The messages of this conversation or thread, oldest first. */
  messages: Message[];
  /** The message the person is replying to with a quote. */
  quote?: string;
  /** Sends `text` here as the person's own message; resolves with its ID once accepted. */
  post: (text: string, quote?: string) => Promise<string | undefined>;
  /** Lets the host say which message the text in the box became. */
  sent: { current?: (message: string) => void };
  /** On Ask me first (schema 141): the reply your assistant wrote on its own in `thread`. */
  waiting?: { thread: string } & WaitingReply;
  focusBox?: () => void; disabled?: boolean;
}): AssistantAsk {
  const { room, reload } = useThreads();
  // A waiting reply this person sent, discarded or took into the box, until the thread list says the same.
  const [left, setLeft] = useState<string>();
  const [asked, setAsked] = useState<Asked | undefined>(() => loadAsked(chat, thread));
  // A draft moved into the message box: whose it is, and what the box held before.
  const [editing, setEditing] = useState<{ reply: string; before: string }>();
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const pending = useRef(false);
  // Messages sent from a draft, until the server has them and the private record is kept.
  const used = useRef(new Map<string, string>());
  const box = useRef({ value, editing }); box.current = { value, editing };
  useEffect(() => { keepAsked(chat, thread, asked); }, [chat, thread, asked]);

  const writing = asked?.state === 'writing' ? asked.reply : undefined;
  // While the assistant writes, read the draft every two seconds, then every six, for ten minutes.
  useEffect(() => {
    if (!writing) return;
    const abort = new AbortController(); let ticks = 0, reading = false;
    const settle = (patch: Partial<Asked>) => setAsked(previous => previous?.reply === writing ? { ...previous, ...patch } : previous);
    const timer = window.setInterval(async () => {
      ticks++;
      if (ticks > 300) { settle({ state: 'failed' }); return; }
      if (reading || (ticks > 60 && ticks % 3 !== 0)) return;
      reading = true;
      try {
        const result = await chatRequest<{ state: string; text: string }>(`/assistant/drafts/${encodeURIComponent(writing)}`, undefined, abort.signal);
        if (abort.signal.aborted) return;
        if (result.state === 'complete') settle(result.text.trim() ? { state: 'ready', text: result.text } : { state: 'failed' });
        else if (!['queued', 'streaming'].includes(result.state)) settle({ state: 'failed' });
      } catch { /* The next read tries again. */ }
      finally { reading = false; }
    }, 2000);
    return () => { abort.abort(); window.clearInterval(timer); };
  }, [writing]);

  // Once a message written by the assistant is sent, keep the private record of who wrote it.
  useEffect(() => {
    sent.current = message => {
      const { editing, value } = box.current;
      if (editing && value.trim()) used.current.set(message, editing.reply);
      setEditing(undefined);
    };
    const unsubscribe = subscribeSent((_, message) => {
      const reply = message && used.current.get(message.id);
      if (!message || !reply) return;
      used.current.delete(message.id);
      // The record also tells a thread that its reply waits no more.
      void chatRequest(`/assistant/drafts/${encodeURIComponent(reply)}/used`, { message: message.id }).then(reload, () => undefined);
    });
    return () => { sent.current = undefined; unsubscribe(); };
  }, [sent]);

  if (!assistant || chat === room) return none;
  const name = assistant.name;
  const mention: Mention = { id: assistant.id, name, kind: 'agent', avatar: assistant.avatar };
  const instruction = assistantRequest(value, assistant.id);
  const latest = Math.max(0, ...messages.map(message => message.sequence ?? 0));

  /** One request at a time; the draft on screen stays until the new one is asked for. */
  const request = async ({ instruction, change, message, previous, quote }: Request) => {
    if (pending.current) return false;
    pending.current = true; setBusy(true); setError('');
    try {
      const result = await chatRequest<{ reply: string }>('/assistant/drafts', { id: crypto.randomUUID(), chat, thread: thread ?? null,
        instruction: previous ? change ?? '' : change ? `${instruction}\nAlso: ${change}` : instruction, message: message ?? null, previous: previous ?? null });
      setAsked({ reply: result.reply, state: 'writing', instruction, change, text: '', sequence: latest, at: Date.now(), quote, answering: message });
      return true;
    } catch (e) { setError(errorMessage(e)); return false; }
    finally { pending.current = false; setBusy(false); }
  };
  const ask = async () => {
    if (instruction === undefined) return false;
    // With nothing to go on but "respond", the draft answers the newest message meant for you.
    const bare = answersLatest(instruction);
    const message = quote ?? (bare ? latestForYou(messages, viewer, direct)?.id : undefined);
    const accepted = await request({ instruction: bare ? '' : instruction, message, quote });
    if (accepted) { change(''); setEditing(undefined); }
    return accepted;
  };
  // A reply written on its own is shown like one you asked for, when none of those is on screen.
  const unasked = !asked && waiting?.state === 'complete' && waiting.text.trim() && waiting.reply !== left ? waiting : undefined;
  const draft: Asked | undefined = asked ?? (unasked && { reply: unasked.reply, state: 'ready', instruction: '', text: unasked.text, sequence: latest, at: 0 });
  const leave = () => { if (unasked) setLeft(unasked.reply); setAsked(undefined); setError(''); focusBox?.(); };
  const send = async () => {
    if (!draft || pending.current) return;
    pending.current = true; setBusy(true); setError('');
    try {
      const message = await post(draft.text, draft.quote);
      if (message) { used.current.set(message, draft.reply); leave(); }
    } catch (e) { setError(errorMessage(e)); }
    finally { pending.current = false; setBusy(false); }
  };
  const edit = () => {
    if (!draft) return;
    setEditing({ reply: draft.reply, before: value }); change(draft.text); leave();
  };
  // Discarding one that waited in a thread tells the thread, so it is not offered again.
  const discard = () => { if (unasked) void discardReply(unasked.thread).then(reload, () => undefined); leave(); };
  // The button that asked leaves with the draft it belonged to, so typing continues in the box.
  const asksAgain = async (next: Request) => { const accepted = await request(next); if (accepted) focusBox?.(); return accepted; };
  const again = () => { if (draft) void asksAgain({ instruction: draft.instruction, change: draft.change, message: draft.answering, quote: draft.quote }); };
  const revise = async (text: string) => draft ? asksAgain({ instruction: draft.instruction, change: text, previous: draft.reply, message: draft.answering, quote: draft.quote }) : false;

  const tool = <Button className="composer-draft" aria-label={`Ask ${name}`} disabled={disabled}
    title={`Ask ${name} to write a reply. Only you see it, and nothing is sent until you send it.`}
    onClick={() => { if (instruction === undefined) change(mentionAssistant(value, mention)); focusBox?.(); }}><AssistantMark size={17} /></Button>;
  const notice = instruction !== undefined
    ? <div className="composer-notice composer-asking" role="status"><AssistantMark /><strong>Asking {name}</strong><span>Only you see this. Nothing is sent {where}.</span></div>
    : namesAssistant(value, mention)
    ? <div className="composer-notice composer-warning" role="status"><strong>This would be sent {where} as written.</strong><span>Did you mean to ask {name}?</span>
        <Button onClick={() => { change(mentionTyped(value, mention)); focusBox?.(); }}>Ask {name} instead</Button></div>
    : editing && value.trim()
    ? <div className="composer-notice" role="status" aria-label={`Draft from ${name}`}><AssistantMark /><strong>Draft from {name}</strong><span>Only you see this. Edit it, then send.</span>
        <Button onClick={() => { change(editing.before); setEditing(undefined); if (editing.reply === left) setLeft(undefined); }}>Discard</Button></div>
    : null;
  const card = draft
    ? <AskCard asked={draft} name={name} where={where} busy={busy} disabled={disabled} error={error}
        answered={draft.answering ? messages.find(message => message.id === draft.answering) : undefined}
        moved={draft.state === 'ready' ? messages.filter(message => message.author !== viewer && (message.sequence ?? 0) > draft.sequence).length : 0}
        actions={{ send: () => void send(), edit, discard, again, revise }} />
    : error ? <p className="chat-error" role="alert">{error} <Button onClick={() => setError('')}>Dismiss</Button></p> : null;
  return { mention, asking: instruction !== undefined, ask, tool, notice, card };
}
