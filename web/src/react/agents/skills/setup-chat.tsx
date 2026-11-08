import { useState, type ReactNode, type RefObject } from 'react';
import { Button } from '../../ui';
import { useMutation, useResource } from '../../requests';
import { askWriter, loadWriter, type ChatMessage, type WriterAnswer, type WriterRequest } from './writer-model';
import './setup-chat.css';

type Shown = ChatMessage & { key: string };

/** The setup chat's state: messages, the composer and the writer's notes. The
 * conversation stays in the browser; each turn sends it again. */
export function useSkillWriting() {
  const writer = useResource(loadWriter);
  const [messages, setMessages] = useState<Shown[]>([]);
  const [notes, setNotes] = useState<string[]>([]);
  const [composer, setComposer] = useState('');
  const sending = useMutation();
  /** Sends the composer; on an answer, `apply` gets it and the composer clears.
   * A failed turn keeps the text so it can be sent again. */
  type Setup = Omit<WriterRequest, 'messages'>;
  const send = (request: Setup | (() => Promise<Setup>), apply: (answer: WriterAnswer) => void) => {
    const text = composer.trim();
    if (!text) return;
    const person: Shown = { key: crypto.randomUUID(), role: 'person', text };
    void sending.run(async () => {
      const history = [...messages, person].slice(-40).map(({ role, text }) => ({ role, text }));
      const setup = typeof request === 'function' ? await request() : request;
      const answer = await askWriter({ ...setup, messages: history });
      setMessages(old => [...old, person, { key: crypto.randomUUID(), role: 'agent', text: answer.reply }]);
      setNotes([...answer.notes, ...answer.missing_apps.map(app => `${app} isn’t one of the apps here, so no skill uses it. An admin can add it.`)]);
      setComposer('');
      apply(answer);
    });
  };
  return { available: writer.data?.available ?? false, loading: !writer.data && !writer.error, guide: writer.data?.guide ?? '',
    messages, notes, composer, setComposer, busy: sending.busy, status: sending.status, send };
}
export type SkillWriting = ReturnType<typeof useSkillWriting>;

/** The chat panel (designs/agent-built-skills 2 and 5). `greeting` opens an
 * empty chat; `extra` sits above the composer (starters, proposed changes). */
export function SetupChat({ name, title = name, subtitle, greeting, writing, submit, extra, input }: {
  name: string; title?: string; subtitle: string; greeting: string; writing: SkillWriting; submit: () => void; extra?: ReactNode;
  input?: RefObject<HTMLTextAreaElement | null>;
}) {
  const { messages, notes, composer, setComposer, busy, status, guide } = writing;
  return <section className="setup-chat" aria-label={title === name ? `Chat with ${name}` : title}>
    <header className="setup-chat-header"><strong>{title}</strong><small>{subtitle}</small></header>
    <ol className="setup-chat-messages" aria-live="polite">
      <li className="setup-chat-message agent">{greeting}</li>
      {messages.map(message => <li key={message.key} className={`setup-chat-message ${message.role}`}>{message.text}</li>)}
      {busy && <li className="setup-chat-message agent" role="status">Writing…</li>}
    </ol>
    {notes.length > 0 && <ul className="setup-chat-notes" aria-label="Notes">{notes.map(note => <li key={note}>{note}</li>)}</ul>}
    {extra}
    <form className="setup-chat-composer" onSubmit={event => { event.preventDefault(); submit(); }}>
      <label className="setup-chat-label" htmlFor="setup-chat-input">Message {name}</label>
      <textarea ref={input} id="setup-chat-input" rows={3} maxLength={8000} value={composer} placeholder={`Tell ${name} what to do or change…`}
        onChange={event => setComposer(event.target.value)}
        onKeyDown={event => { if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) { event.preventDefault(); submit(); } }} />
      <Button primary type="submit" disabled={busy || !composer.trim()}>{busy ? 'Sending…' : 'Send'}</Button>
    </form>
    {status && <p className="setup-chat-error" role="alert">{status}</p>}
    {guide && <details className="setup-chat-guide"><summary>What {name} is told while setting up</summary><pre>{guide}</pre></details>}
  </section>;
}
