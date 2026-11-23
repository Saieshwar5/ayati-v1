import { useState } from 'react';
import { Button } from '../ui';
import { AssistantMark } from './assistant-threads';
import { decodeMentions } from './mentions';
import type { Message } from './model';
import type { Asked } from './assistant-ask-model';
import './assistant-ask.css';

export type AskCardActions = {
  /** Posts the draft as the person's own message. */
  send: () => void;
  /** Moves the draft into the message box. */
  edit: () => void;
  discard: () => void;
  /** Asks again with the same words, reading the conversation as it is now. */
  again: () => void;
  /** Asks for a change to this draft; true once the assistant was asked. */
  revise: (change: string) => Promise<boolean>;
};

/** What your assistant is writing, or wrote, for you to send here. It sits
 * above the message box and only you get it; nothing in it is in the
 * conversation until you send it. */
export function AskCard({ asked, name, where, answered, moved, busy, disabled, error, actions }: {
  asked: Asked; name: string;
  /** Where a sent draft goes: "in #sales", "to Meera Iyer". */
  where: string;
  /** The message the draft answers, when it is still loaded. */
  answered?: Message;
  /** Messages from others since you asked. */
  moved: number; busy: boolean; disabled: boolean; error: string; actions: AskCardActions;
}) {
  const [change, setChange] = useState('');
  if (asked.state === 'writing') return <section className="assistant-ask" aria-label={`${name} is writing, only you see this`}>
    <p className="assistant-ask-line" role="status">
      <AssistantMark /><strong>{name} is writing a reply for you…</strong><span>Only you see this. You can keep writing.</span>
      <Button onClick={actions.discard}>Cancel</Button>
    </p>
  </section>;
  if (asked.state === 'failed') return <section className="assistant-ask" aria-label={`${name}, only you see this`}>
    <p className="assistant-ask-line" role="alert">
      <AssistantMark /><strong>{name} could not write a reply</strong><span>Nothing was sent.</span>
      <Button disabled={busy || disabled} onClick={actions.again}>Try again</Button><Button onClick={actions.discard}>Dismiss</Button>
    </p>
    {error && <p className="chat-error" role="alert">{error}</p>}
  </section>;
  const submit = async () => { if (change.trim() && await actions.revise(change.trim())) setChange(''); };
  return <section className="assistant-ask" aria-label={`Draft from ${name}, only you see this`}>
    <p className="assistant-ask-line">
      <AssistantMark /><strong>Draft from {name}</strong><span>Only you see this · Will be sent as you {where}</span>
    </p>
    {asked.instruction && <p className="assistant-ask-asked">You asked: {asked.instruction}{asked.change && <> · Then: {asked.change}</>}</p>}
    {answered && <p className="assistant-ask-quote">Answering {answered.name ?? 'this message'}: “{excerpt(answered.content)}”</p>}
    {moved > 0 && <p className="assistant-ask-moved" role="status">
      <strong>{moved === 1 ? '1 new message' : `${moved} new messages`} since {name} wrote this.</strong>
      <Button disabled={busy || disabled} onClick={actions.again}>Update draft</Button>
    </p>}
    <p className="assistant-ask-text">{asked.text}</p>
    {error && <p className="chat-error" role="alert">{error}</p>}
    <div className="assistant-ask-actions">
      <Button primary={!moved} disabled={busy || disabled} onClick={actions.send}>Send as me</Button>
      <Button disabled={busy || disabled} onClick={actions.edit}>Edit</Button>
      <Button disabled={busy} onClick={actions.discard}>Discard</Button>
    </div>
    <form className="assistant-ask-change" onSubmit={event => { event.preventDefault(); void submit(); }}>
      <input aria-label={`Ask ${name} to change it`} placeholder={`Ask ${name} to change it…`} maxLength={1000} value={change} disabled={disabled}
        onChange={event => setChange(event.target.value)} />
      <Button type="submit" disabled={busy || disabled || !change.trim()}>Change</Button>
    </form>
  </section>;
}

function excerpt(content: string): string {
  const text = decodeMentions(content).text.replace(/\s+/g, ' ').trim();
  return text.length > 160 ? `${text.slice(0, 159)}…` : text;
}
